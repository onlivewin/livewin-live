use lazy_static::lazy_static;

use config::Config;
use config::File;
use serde::Deserialize;
use std::sync::RwLock;

lazy_static! {
    static ref SETTINGS: RwLock<Settings> = {
        let conf = Config::builder()
            .add_source(File::with_name("conf.yaml"))
            .build()
            .unwrap();

        let s: Settings = conf.try_deserialize().unwrap();
        RwLock::new(s)
    };
}

pub fn get_setting() -> Settings {
    let lock = SETTINGS.read().unwrap();
    lock.to_owned()
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub rtmp: Rtmp,
    pub hls: Hls,
    pub http_flv: HTTPFLV,
    pub redis: String,
    pub auth_enable: bool,
    pub log_level: String,
    pub full_gop: bool,
    pub flv:Flv,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Rtmp {
    pub port: i32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Flv {
    pub enable: bool,
    pub data_path: String,
}


#[derive(Debug, Deserialize, Clone)]
pub struct Hls {
    pub enable: bool,
    pub port: i32,
    pub ts_duration: u64,
    pub data_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HTTPFLV {
    pub enable: bool,
    pub port: i32,
}
