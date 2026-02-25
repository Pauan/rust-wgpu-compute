import rust from "@wasm-tool/rollup-plugin-rust";

export default {
    input: {
        index: "./Cargo.toml",
    },
    output: {
        dir: "dist/js",
        format: "es",
        sourcemap: true,
    },
    plugins: [
        rust({
            optimize: {
                release: false
            },
            extraArgs: {
                rustc: ["--cfg", "web_sys_unstable_apis"],
            },
        }),
    ],
};
