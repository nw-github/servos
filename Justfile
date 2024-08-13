test:
    cargo build --bin init
    mkdir -p initrd/bin
    rsync target/riscv64imac-unknown-none-elf/debug/init initrd/bin/init
    python mkfs.py initrd initrd.img
    cargo run --bin servos
