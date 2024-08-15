test:
    cargo b -q --bin init
    cargo b -q --bin ls
    cargo b -q --bin sh
    cargo b -q --bin tests
    cargo b -q --bin bf
    cargo b -q --bin cat
    cargo b -q --bin shutdown

    mkdir -p initrd/bin

    rsync target/riscv64imac-unknown-none-elf/debug/init initrd/bin/init
    rsync target/riscv64imac-unknown-none-elf/debug/ls initrd/bin/ls
    rsync target/riscv64imac-unknown-none-elf/debug/sh initrd/bin/sh
    rsync target/riscv64imac-unknown-none-elf/debug/tests initrd/bin/tests
    rsync target/riscv64imac-unknown-none-elf/debug/bf initrd/bin/bf
    rsync target/riscv64imac-unknown-none-elf/debug/cat initrd/bin/cat
    rsync target/riscv64imac-unknown-none-elf/debug/shutdown initrd/bin/shutdown

    python mkfs.py initrd initrd.img
    cargo r --bin servos
