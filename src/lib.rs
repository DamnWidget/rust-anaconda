// Copyright 2016 Oscar Campos <damnwidget@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

extern crate libc;
extern crate rustfmt;
extern crate getopts;

use libc::{c_char, c_int};

use rustfmt::{Input, Summary, run};
use rustfmt::config::{Config, WriteMode};

use std::{env, error, mem};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::ffi::{CString, CStr};
use std::path::{Path, PathBuf};

type FmtError = Box<error::Error + Send + Sync>;
type FmtResult<T> = std::result::Result<T, FmtError>;

fn match_cli_path_or_file(config_path: Option<PathBuf>,
                          input_file: &Path)
                          -> FmtResult<(Config, Option<PathBuf>)> {

    if let Some(config_file) = config_path {
        let (toml, path) = try!(resolve_config(config_file.as_ref()));
        if path.is_some() {
            return Ok((toml, path));
        }
    }
    resolve_config(input_file)
}

fn resolve_config(dir: &Path) -> FmtResult<(Config, Option<PathBuf>)> {
    let path = try!(lookup_project_file(dir));
    if path.is_none() {
        return Ok((Config::default(), None));
    }
    let path = path.unwrap();
    let mut file = try!(File::open(&path));
    let mut toml = String::new();
    try!(file.read_to_string(&mut toml));
    Ok((Config::from_toml(&toml), Some(path)))
}

fn lookup_project_file(dir: &Path) -> FmtResult<Option<PathBuf>> {
    let mut current = if dir.is_relative() {
        try!(env::current_dir()).join(dir)
    } else {
        dir.to_path_buf()
    };

    current = try!(fs::canonicalize(current));

    loop {
        let config_file = current.join("rustfmt.toml");
        match fs::metadata(&config_file) {
            // Only return if it's a file to handle the unlikely situation of a directory named
            // `rustfmt.toml`.
            Ok(ref md) if md.is_file() => return Ok(Some(config_file)),
            // Return the error if it's something other than `NotFound`; otherwise we didn't find
            // the project file yet, and continue searching.
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    return Err(FmtError::from(e));
                }
            }
            _ => {}
        }

        // If the current directory has no parent, we're done searching.
        if !current.pop() {
            return Ok(None);
        }
    }
}

pub fn execute(buffer: String, cfg_path: Option<String>) -> i32 {
    let config_path: Option<PathBuf> = cfg_path
        .map(PathBuf::from)
        .and_then(|dir| {
            if dir.is_file() {
                return dir.parent().map(|v| v.into());
            }
            Some(dir)
        });

    // try to read config from local directory
    let (mut config, _) = match_cli_path_or_file(config_path, &env::current_dir().unwrap())
        .expect("Error resolving config");

    // write_mode is alwais Plain for anaconda_rust
    config.write_mode = WriteMode::Plain;

    // run the command and return status code
    process_summary(run(Input::Text(buffer), &config))
}

fn process_summary(error_summary: Summary) -> i32 {
    let status_code: i32;
    if error_summary.has_operational_errors() {
        status_code = 1
    } else if error_summary.has_parsing_errors() {
        status_code = 2
    } else if error_summary.has_formatting_errors() {
        status_code = 3
    } else {
        assert!(error_summary.has_no_errors());
        status_code = 0
    }

    // flush standard output
    std::io::stdout().flush().unwrap();
    // return the excution code
    status_code
}

// FFI related

/// This function converts a C char * string into a safe Rust String
/// It assures that the c_str is not null using assert! macro so you
/// must be certain that yo never pass null strings to any of the
/// exported functions
fn c_str_to_safe_string(c_str: *const libc::c_char) -> String {
    unsafe {
        assert!(!c_str.is_null());
        CStr::from_ptr(c_str).to_string_lossy().into_owned()
    }
}

/// Converts a Rust String into a C char * and returns a pointer
/// to it's inner memory
///
/// WARNING: this function forgets about the allocated memory so
/// YOU MUST MAKE SURE to delete this memory yourself, there is
/// a convenience exported function to do that, you can just
/// call `free_c_char_mem` with the C string as parameter to
/// free the allocated memory from your C compatible code
fn to_c_str(s: String) -> *mut c_char {
    let s = CString::new(s).unwrap();
    let p = s.as_ptr();
    mem::forget(s);
    p as *mut _
}

/// Return this crate version as a C string
///
/// NOTE: You should free the allocated string memory after is not need anymore
#[no_mangle]
pub extern fn get_version() -> *mut c_char {
    to_c_str(String::from(option_env!("CARGO_PKG_VERSION").unwrap_or("unknown")))
}

/// This function can be used to free memory allocated by Rust
///
/// You can also free the memory in your C compatible app calling
/// the stdlib free function for example
#[no_mangle]
pub extern fn free_c_char_mem(c: *mut c_char) {
    unsafe {
        if c.is_null() {
            return;
        }

        let c_str: &CStr = CStr::from_ptr(c);
        let bytes_len: usize = c_str.to_bytes_with_nul().len();
        let _: Vec<c_char> = Vec::from_raw_parts(c, bytes_len, bytes_len);
    }
}

/// Format the passed buffer using librustfmt and return back an operation
/// status code, librustfmt uses the standard output to print the formating
/// results so you should capture it in you C level code.
///
/// No memory need to be freed after use this function as it is automatically
/// handled by Rust itself
#[no_mangle]
pub extern fn format(code: *const c_char, path: *const c_char) ->  c_int {
    let config_path: Option<String> = Some(c_str_to_safe_string(path));
    let buffer = c_str_to_safe_string(code);
    execute(buffer, config_path)
}
