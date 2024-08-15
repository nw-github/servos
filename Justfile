test:
    mkdir -p initrd/bin

    cargo b --bin init
    rsync target/riscv64imac-unknown-none-elf/debug/init initrd/bin/init

    cargo b --bin ls
    rsync target/riscv64imac-unknown-none-elf/debug/ls initrd/bin/ls

    cargo b --bin sh
    rsync target/riscv64imac-unknown-none-elf/debug/sh initrd/bin/sh

    cargo b --bin tests
    rsync target/riscv64imac-unknown-none-elf/debug/tests initrd/bin/tests

    cargo b --bin bf
    rsync target/riscv64imac-unknown-none-elf/debug/bf initrd/bin/bf

    python mkfs.py initrd initrd.img
    cargo r --bin servos
