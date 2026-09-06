fn main() {
    tonic_build::configure()
        .build_server(true)
        .compile_protos(&["proto/hello.proto"], &["proto"])
        .expect("failed to compile protobuf definitions");
}
