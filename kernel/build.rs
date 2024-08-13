fn main() {
    println!("cargo:rustc-link-arg=-Tkernel/src/kernel.ld");
    println!("cargo:rustc-link-arg=--omagic");
}
