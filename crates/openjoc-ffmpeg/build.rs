fn main() {
    println!("cargo:rerun-if-changed=src/interop.c");
    if std::env::var_os("CARGO_FEATURE_FFMPEG").is_none() {
        return;
    }

    let avutil = pkg_config::Config::new()
        .atleast_version("61.0.0")
        .probe("libavutil")
        .expect("FFmpeg 9+ libavutil development files are required");
    let avcodec = pkg_config::Config::new()
        .atleast_version("63.0.0")
        .probe("libavcodec")
        .expect("FFmpeg 9+ libavcodec development files are required");
    let avformat = pkg_config::Config::new()
        .atleast_version("63.0.0")
        .probe("libavformat")
        .expect("FFmpeg 9+ libavformat development files are required");

    let mut build = cc::Build::new();
    build
        .file("src/interop.c")
        .warnings(true)
        .flag_if_supported("-std=c11");
    for path in avutil
        .include_paths
        .iter()
        .chain(&avcodec.include_paths)
        .chain(&avformat.include_paths)
    {
        build.include(path);
    }
    build.compile("openjoc_ffmpeg_interop");
}
