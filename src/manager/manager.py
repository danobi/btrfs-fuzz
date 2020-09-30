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


def get_vm_args(img, state_dir):
    c = []

    c.append("podman run")
    c.append("-it")
    c.append("--privileged")
    c.append(f"-v {state_dir}:/state")
    c.append(img)

    return c


class Manager:
    def __init__(self, img, state_dir):
        # Which docker image to use
        self.img = img

        # Where the state dir is on host
        self.state_dir = state_dir

        self.prompt_regex = "root@.*#"

        self.vm = None

    def spawn_vm(self):
        cmd = " ".join(get_vm_args(self.img, self.state_dir))
        self.vm = pexpect.spawn(cmd, encoding="utf-8")

        # Pipe everything the child prints to our stdout
        self.vm.logfile_read = sys.stdout

        self.vm.expect(self.prompt_regex)

    def run_and_wait(self, cmd):
        """Run a command in the VM and wait until the command completes"""
        self.vm.sendline(cmd)
        self.vm.expect(self.prompt_regex)

    def run(self):
        # Start the VM (could take a few seconds)
        self.spawn_vm()

        # Set core pattern
        self.run_and_wait('/bin/bash -c "echo core > /proc/sys/kernel/core_pattern"')

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
