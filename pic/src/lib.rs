extern crate libc;

use libc::c_int;
use libc::c_char;
use std::ffi::{CString};

pub fn keyframe_to_jpg(video:Vec<u8>,file_name:String)->bool {
    let file_name = CString::new(file_name).unwrap();
    unsafe {
       match video_decode(video.as_ptr(),video.len() as i32,file_name.as_ptr() as *const c_char) {
            0=>true,
            _=>false,
       }
    }
}

extern "C" {
    pub fn  video_decode(data:*const u8,size:c_int,file_name:* const c_char)->c_int;
}