import os
import sys

import pexpect

FORKSERVER_DEATH = "Unable to communicate with fork server"


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


def get_cmd_args():
    c = []

    c.append("/usr/local/bin/afl-fuzz")
    c.append("-m 500")
    c.append("-i /state/input")
    c.append("-o /state/output")
    c.append("--")
    c.append("/btrfs-fuzz/runner")
    c.append("--current-dir /state/current")

    return c


def get_docker_args(img, state_dir):
    c = []

    c.append("podman run")
    c.append("-it")
    c.append("--privileged")
    c.append(f"-v {state_dir}:/state")
    c.append(img)

    return c


def get_nspawn_args(fsdir, state_dir):
    c = []

    abs_fsdir_path = os.path.abspath(fsdir)
    abs_state_dir = os.path.abspath(state_dir)

    c.append("sudo systemd-nspawn")
    c.append(f"--directory {fsdir}")
    c.append("--read-only")
    c.append("--machine btrfs-fuzz")
    c.append(f"--bind={abs_state_dir}:/state")
    c.append("--chdir=/btrfs-fuzz")

    # Map into the container /dev/kvm so qemu can run faster
    c.append(f"--bind=/dev/kvm:/dev/kvm")

    return c


class Manager:
    def __init__(self, img, state_dir, nspawn=False):
        """Initialize Manager
        img: Name of docker image to run
        state_dir: Path to directory to map into /state inside VM
        nspawn: Treat `img` as the path to a untarred filesystem and use systemd-nspawn
             to start container
        """
        # Which docker image to use
        self.img = img

        # Where the state dir is on host
        self.state_dir = state_dir

        self.nspawn = nspawn

        self.prompt_regex = "root@.*#"

        self.vm = None

    def spawn_vm(self):
        if self.nspawn:
            args = get_nspawn_args(self.img, self.state_dir)
        else:
            args = get_docker_args(self.img, self.state_dir)
        cmd = " ".join(args)

        self.vm = pexpect.spawn(cmd, encoding="utf-8")

        # Pipe everything the child prints to our stdout
        self.vm.logfile_read = sys.stdout

        self.vm.expect(self.prompt_regex)

        # For docker images we rely on the ENTRYPOINT directive. For nspawn we
        # have to do it ourselves
        if self.nspawn:
            self.run_and_wait("./entry.sh", disable_timeout=True)

    def run_and_wait(self, cmd, disable_timeout=False):
        """Run a command in the VM and wait until the command completes"""
        self.vm.sendline(cmd)

        if disable_timeout:
            self.vm.expect(self.prompt_regex, timeout=None)
        else:
            self.vm.expect(self.prompt_regex)

    def run(self):
        # Start the VM (could take a few seconds)
        self.spawn_vm()

        # Set core pattern
        self.run_and_wait("echo core > /proc/sys/kernel/core_pattern")

        # Start running fuzzer
        while True:
            cmd = get_cmd_env_vars()
            cmd.extend(get_cmd_args())
            self.vm.sendline(" ".join(cmd))

            expected = [FORKSERVER_DEATH, self.prompt_regex]
            idx = self.vm.expect(expected, timeout=None)
            if idx == 0:
                print("Detected forkserver death, probably caused by BUG()")

                # TODO: look at last-n log and see if we hit any BUG()s
                # recently
                print("TODO handle")
                break
            elif idx == 1:
                print("Unexpected fuzzer exit. Not continuing.")
                break
            else:
                raise RuntimeError(f"Unknown expected idx={idx}")
