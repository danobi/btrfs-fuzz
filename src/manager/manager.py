import asyncio
import os
import signal
import shutil
import sys
import threading
import time
import uuid

import pexpect

MASTER_NAME = "master"
KERNEL_PANIC_STR = "Kernel panic"


def get_secondary_name(idx):
    return f"secondary_{idx}"


def get_cmd_env_vars():
    e = []

    # We didn't build with the afl toolchain so our binary is not watermarked
    e.append("AFL_SKIP_BIN_CHECK=1")

    # Help debug crashes in our runner
    e.append("AFL_DEBUG_CHILD_OUTPUT=1")

    # Our custom mutator only fuzzes the FS metadata. Anything else is
    # ineffective
    e.append("AFL_CUSTOM_MUTATOR_LIBRARY=/btrfs-fuzz/libmutator.so")
    e.append("AFL_CUSTOM_MUTATOR_ONLY=1")

    # The custom mutator doesn't append or delete bytes. Trimming also messes
    # with deserializing input so, don't trim.
    e.append("AFL_DISABLE_TRIM=1")

    # Autoresume work
    e.append("AFL_AUTORESUME=1")

    return e


def get_cmd_args(master=False, secondary=None):
    """Get arguments to invoke AFL with

    Note `master` and `secondary` cannot both be specified.

    master: If true, get arguments for parallel fuzzing master node
    secondary: If specified, the integer value is the secondary instance number.
               This function will then return arguments for parallel fuzzing
               secondary node.
    """
    if master and secondary is not None:
        raise RuntimeError("Cannot specify both master and secondary arguments")

    c = []

    c.append("/usr/local/bin/afl-fuzz")
    c.append("-m 500")
    c.append("-i /state/input")
    c.append("-o /state/output")

    # See bottom of
    # https://github.com/AFLplusplus/AFLplusplus/blob/stable/docs/power_schedules.md
    if master:
        c.append(f"-M {MASTER_NAME}")
        c.append("-p exploit")
    elif secondary is not None:
        c.append(f"-S {get_secondary_name(secondary)}")

        AFL_SECONDARY_SCHEDULES = ["coe", "fast", "explore"]
        idx = secondary % len(AFL_SECONDARY_SCHEDULES)
        c.append(f"-p {AFL_SECONDARY_SCHEDULES[idx]}")

    c.append("--")
    c.append("/btrfs-fuzz/runner")

    return c


def get_docker_args(img, state_dir):
    c = []

    c.append("podman run")
    c.append("-it")
    c.append("--rm")
    c.append("--privileged")
    c.append(f"-v {state_dir}:/state")
    c.append(img)

    return c


def get_nspawn_args(fsdir, state_dir, machine_idx):
    c = []

    abs_fsdir_path = os.path.abspath(fsdir)
    abs_state_dir = os.path.abspath(state_dir)

    c.append("sudo")
    c.append("SYSTEMD_NSPAWN_LOCK=0")
    c.append("systemd-nspawn")
    c.append(f"--directory {fsdir}")
    c.append(f"--bind={abs_state_dir}:/state")
    c.append("--chdir=/btrfs-fuzz")
    c.append(f"--machine btrfs-fuzz-{machine_idx}")

    # Map into the container /dev/kvm so qemu can run faster
    c.append(f"--bind=/dev/kvm:/dev/kvm")

    return c


class VM:
    """One virtual machine instance"""

    def __init__(self, p, args, state_dir, needs_vm_entry=False, name=None):
        """Initialize VM
        p: An already spawned `pexpect.spawn` VM instance. Nothing should be
           running in the VM yet.
        args: Arguments to invoke AFL (string)
        state_dir: State directory for fuzzing session (could be shared between
                   multiple VMs)
        needs_vm_entry: If true, the container has been entered but the VM has
                        not been spawned yet. Pass true to also run ./entry.sh
        name: Name of the VM instance. Only needs to be specified if running
              multiple VMs (ie parallel mode)
        """
        self.vm = p
        self.args = args
        self.state_dir = state_dir
        self.needs_vm_entry = needs_vm_entry
        self.name = name
        self.prompt_regex = "root@.*#"

    async def run_and_wait(self, cmd, disable_timeout=False):
        """Run a command in the VM and wait until the command completes"""
        self.vm.sendline(cmd)

        if disable_timeout:
            await self.vm.expect(self.prompt_regex, timeout=None, async_=True)
        else:
            await self.vm.expect(self.prompt_regex, async_=True)

    def handle_kernel_panic(self):
        """Save the current input if it triggers a kernel panic"""
        output_dir = f"{self.state_dir}/output"
        if self.name is not None:
            output_dir += f"/{self.name}"

        cur_input = os.path.abspath(f"{output_dir}/.cur_input")
        dest = os.path.abspath(f"{self.state_dir}/known_crashes/{uuid.uuid4()}")
        shutil.copy(cur_input, dest)

    async def _run(self):
        # `self.p` should not have been `expect()`d upon yet so we need to wait
        # until a prompt is ready
        await self.vm.expect(self.prompt_regex, async_=True)

        if self.needs_vm_entry:
            await self.run_and_wait("./entry.sh", disable_timeout=True)

        # Set core pattern
        await self.run_and_wait("echo core > /proc/sys/kernel/core_pattern")

        # Start running fuzzer
        self.vm.sendline(self.args)

        expected = [self.prompt_regex, KERNEL_PANIC_STR]
        idx = await self.vm.expect(expected, timeout=None, async_=True)
        if idx == 0:
            print(
                "Unexpected fuzzer exit. Is there a bug with the runner? Not continuing."
            )
        elif idx == 1:
            print("Detected kernel panic!")
            self.handle_kernel_panic()
        else:
            raise RuntimeError(f"Unknown expected idx={idx}")

    async def run(self):
        try:
            await self._run()
        except asyncio.CancelledError:
            # podman won't clean up after us unless we kill the entire process tree
            pid = self.vm.pid
            print(f"Sending SIGKILL to all pids in pid={pid} tree")
            os.system(f"pkill -KILL -P {pid}")


class Manager:
    def __init__(self, img, state_dir, nspawn=False, parallel=None):
        """Initialize Manager
        img: Name of docker image to run
        state_dir: Path to directory to map into /state inside VM
        nspawn: Treat `img` as the path to a untarred filesystem and use systemd-nspawn
             to start container
        parallel: If specified, run parallel fuzzing on provided # of CPUs. -1 means all,
                  default is 1.
        """
        # Which docker image to use
        self.img = img

        # Where the state dir is on host
        self.state_dir = state_dir

        self.nspawn = nspawn
        self.parallel = parallel

        self.vm = None

        # Use to create uniquely named machinectl names
        self.machine_idx = 0

    def spawn_vm(self, display):
        """Spawn a single VM

        display: Print child output to stdout

        Returns a `pexpect.spawn` instance
        """
        if self.nspawn:
            args = get_nspawn_args(self.img, self.state_dir, self.machine_idx)
            self.machine_idx += 1
        else:
            args = get_docker_args(self.img, self.state_dir)
        cmd = " ".join(args)

        p = pexpect.spawn(cmd, encoding="utf-8")

        # Pipe everything the child prints to our stdout
        if display:
            p.logfile_read = sys.stdout

        return p

    def prep_one(self, master=False, secondary=None):
        """Run one fuzzer instance

        master: If true, spawn the master instance
        secondary: If specified, the integer number of the secondary instance

        Returns a `VM` instance
        """
        # Start the VM (could take a few seconds)
        #
        # Only display output if master instance or sole instance in non-parallel
        # mode
        p = self.spawn_vm(master or (not master and not secondary))

        # For docker images we rely on the ENTRYPOINT directive. For nspawn we
        # have to do it ourselves
        if self.nspawn:
            needs_vm_entry = True
        else:
            needs_vm_entry = False

        cmd = get_cmd_env_vars()
        cmd.extend(get_cmd_args(master, secondary))

        if master:
            name = MASTER_NAME
        elif secondary is not None:
            name = get_secondary_name(secondary)
        else:
            name = None

        return VM(
            p, " ".join(cmd), self.state_dir, needs_vm_entry=needs_vm_entry, name=name
        )

    async def run_parallel(self, nr_cpus):
        tasks = []
        for i in range(nr_cpus):
            if i == 0:
                name = f"btrfs-fuzz-{MASTER_NAME}"
                vm = self.prep_one(master=True)
            else:
                name = f"btrfs-fuzz-{get_secondary_name(i)}"
                vm = self.prep_one(secondary=i)

            if sys.version_info < (3, 7):
                t = asyncio.ensure_future(vm.run())
            else:
                t = asyncio.create_task(vm.run(), name=name)
            tasks.append(t)

        # Now manage all the running tasks -- if any die, we'll error out
        # for now. In the future we should log the crash and respawn the
        # thread.
        while True:
            triggering_task = None
            exit = False
            for t in tasks:
                if t.done():
                    if sys.version_info < (3, 8):
                        name = "?"
                    else:
                        name = t.get_name()
                    print(f"Task={name} unexpectedly exited. Exiting now.")
                    triggering_task = t
                    exit = True
                    break

            if exit:
                # Cancel all the outstanding tasks so we don't leak VMs
                for t in [t for t in tasks if not t.done()]:
                    t.cancel()

                print("All other tasks cancelled")

                # Print out stacktrace from task that triggered the exit
                t = triggering_task
                exc = t.exception()
                if exc:
                    if sys.version_info < (3, 8):
                        name = "?"
                    else:
                        name = t.get_name()
                    print(f"Exception from {name}: {exc}")
                    t.print_stack()

                break

            await asyncio.sleep(1)

    async def _run(self):
        run_parallel = False
        if self.parallel is not None:
            if self.parallel == -1:
                run_parallel = True
                nr_cpus = len(os.sched_getaffinity(0))
            elif self.parallel > 1:
                run_parallel = True
                nr_cpus = self.parallel

        if run_parallel:
            await self.run_parallel(nr_cpus)
        else:
            await self.prep_one().run()

    def run(self):
        def cancel_all_tasks():
            if sys.version_info < (3, 7):
                all_tasks = asyncio.Task.all_tasks()
            else:
                all_tasks = asyncio.all_tasks()

            for t in all_tasks:
                t.cancel()

        # If we don't cancel the outstanding tasks, the containers leak
        loop = asyncio.get_event_loop()
        loop.add_signal_handler(signal.SIGINT, cancel_all_tasks)
        loop.add_signal_handler(signal.SIGTERM, cancel_all_tasks)
        loop.add_signal_handler(signal.SIGHUP, cancel_all_tasks)

        try:
            loop.run_until_complete(self._run())
        except asyncio.CancelledError:
            pass
