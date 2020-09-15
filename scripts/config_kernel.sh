#!/bin/bash
#
# Configure kernel for btrfs fuzzing
#
# Run this inside kernel source tree root.

set -eu

# Enable CONFIG_KCOV but don't instrument everything. We don't want any
# noise from anything outside btrfs
./scripts/config \
    -e KCOV \
    -d KCOV_INSTRUMENT_ALL \
    -e KCOV_ENABLE_COMPARISONS

# Enable KCOV instrumentation for btrfs
find fs/btrfs -name Makefile \
    | xargs -L1 -I {} \
    bash -c 'grep -q KCOV_INSTRUMENT {} || echo "KCOV_INSTRUMENT := y" >> {}'

# Apply syzkaller recommended configs. See:
# https://github.com/google/syzkaller/blob/master/docs/linux/kernel_configs.md
./scripts/config \
    -e DEBUG_FS \
    -e DEBUG_INFO \
    -e KALLSYMS \
    -e KALLSYMS_ALL \
    -e NAMESPACES \
    -e UTS_NS \
    -e IPC_NS \
    -e PID_NS \
    -e NET_NS \
    -e USER_NS \
    -e CGROUP_PIDS \
    -e MEMCG \
    -e CONFIGFS_FS \
    -e SECURITYFS \
    -e KASAN \
    -e KASAN_INLINE \
    -e WARNING \
    -e FAULT_INJECTION \
    -e FAULT_INJECTION_DEBUG_FS \
    -e FAILSLAB \
    -e FAIL_PAGE_ALLOC \
    -e FAIL_MAKE_REQUEST \
    -e FAIL_IO_TIMEOUT \
    -e FAIL_FUTEX \
    -e LOCKDEP \
    -e PROVE_LOCKING \
    -e DEBUG_ATOMIC_SLEEP \
    -e PROVE_RCU \
    -e DEBUG_VM \
    -e REFCOUNT_FULL \
    -e FORTIFY_SOURCE \
    -e HARDENED_USERCOPY \
    -e LOCKUP_DETECTOR \
    -e SOFTLOCKUP_DETECTOR \
    -e HARDLOCKUP_DETECTOR \
    -e BOOTPARAM_HARDLOCKUP_PANIC \
    -e DETECT_HUNG_TASK \
    -e WQ_WATCHDOG \
    --set-val DEFAULT_HUNG_TASK_TIMEOUT 140 \
    --set-val RCU_CPU_STALL_TIMEOUT 100 \
    -e UBSAN \
    -d RANDOMIZE_BASE

# Apply virtme required configs
./scripts/config \
    -e VIRTIO \
    -e VIRTIO_PCI \
    -e VIRTIO_MMIO \
    -e NET \
    -e NET_CORE \
    -e NETDEVICES \
    -e NETWORK_FILESYSTEMS \
    -e INET \
    -e NET_9P \
    -e NET_9P_VIRTIO \
    -e 9P_FS \
    -e VIRTIO_NET \
    -e VIRTIO_CONSOLE \
    -e DEVTMPFS \
    -e SCSI_VIRTIO \
    -e BINFMT_SCRIPT \
    -e TMPFS \
    -e UNIX \
    -e TTY \
    -e VT \
    -e UNIX98_PTYS \
    -e WATCHDOG \
    -e WATCHDOG_CORE \
    -e I6300ESB_WDT \
    -e BLOCK \
    -e SCSI_gLOWLEVEL \
    -e SCSI \
    -e SCSI_VIRTIO \
    -e BLK_DEV_SD \
    -e VIRTIO_BALLOON \
    -d CMDLINE_OVERRIDE \
    -d UEVENT_HELPER \
    -d EMBEDDED \
    -d EXPERT \
    -d MODULE_SIG_FORCE

# Build btrfs module in-kernel
./scripts/config -e BTRFS_FS

# Enable btrfs checks
./scripts/config \
    -e BTRFS_FS_CHECK_INTEGRITY \
    -e BTRFS_FS_RUN_SANITY_TESTS \
    -e BTRFS_DEBUG \
    -e BTRFS_ASSERT \
    -e BTRFS_FS_REF_VERIFY

# Setting previous configs may result in more sub options being available,
# so set all the new available ones to default as well.
make olddefconfig
