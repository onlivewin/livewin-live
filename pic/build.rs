use std::env;

fn main() {
    let mut build = cc::Build::new();
    //build.include("/usr/local/ffmpeg/include");
   // println!("cargo:rustc-link-search=native=/usr/local/ffmpeg/lib/");
    println!("cargo:rustc-link-lib=dylib=avcodec");
    println!("cargo:rustc-link-lib=dylib=avutil");
    println!("cargo:rustc-link-lib=dylib=avformat");
    println!("cargo:rustc-link-lib=dylib=swresample");

    build.file("src/pic.c");
    build.pic(true);
    build.flag("-lavcodec");
    build.flag("-lavutil");
    build.flag("-lavformat");
    build.flag("-lswresample"); 
    
    build.opt_level(3);
    build.compile("pic");
}
