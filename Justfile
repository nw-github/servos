test:
    mkdir -p initrd/bin

    cargo build --bin init
    rsync target/riscv64imac-unknown-none-elf/debug/init initrd/bin/init

    cargo build --bin ls
    rsync target/riscv64imac-unknown-none-elf/debug/ls initrd/bin/ls

    python mkfs.py initrd initrd.img
    cargo run --bin servos
