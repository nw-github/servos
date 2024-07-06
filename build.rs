fn main() {
    println!("cargo:rustc-link-arg=-Tsrc/kernel.ld");
    println!("cargo:rustc-link-arg=--omagic");
}
