fn main() {
    cc::Build::new()
        .file("src/proto_hook.c")
        .compile("proto_hook");
    println!("cargo:rerun-if-changed=src/proto_hook.c");
}
