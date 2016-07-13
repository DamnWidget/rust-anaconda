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

use libc::{c_char, c_uchar, c_int};

use rustfmt::{Input, Summary, run};
use rustfmt::config::{Config, WriteMode};

use std::{env, error};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::ffi::{CString, CStr};
use std::path::{Path, PathBuf};

use getopts::{Options};

type FmtError = Box<error::Error + Send + Sync>;
type FmtResult<T> = std::result::Result<T, FmtError>;

fn c_str_to_safe_string(c_str: *const libc::c_char) -> String {
    unsafe {
        assert!(!c_str.is_null());
        CStr::from_ptr(c_str).to_string_lossy().into_owned()
    }
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

pub fn make_opts() -> Options {
    let mut opts = Options::new();
    opts.optopt("",
                 "config-path",
                 "Recursively searches the given path for the rustfmt.toml config file. If not \
                  found reverts to the input file path",
                 "[Path for the configuration file}");
    opts.optflag("", "skip-children", "don't reformat child modules");
    opts
}

pub fn execute(args: Vec<String>, buffer: String) -> i32 {
    let opts: Options = make_opts();

    let matches = match opts.parse(args) {
        Ok(m) => m,
        Err(e) => {
            usage(&opts, &e.to_string());
            return -1;
        }
    };

    // get common options from matches
    let skip_children = matches.opt_present("skip-children");
    println!("skip_children: {}", skip_children);
    let config_path: Option<PathBuf> = matches.opt_str("config-path")
        .map(PathBuf::from)
        .and_then(|dir| {
            if dir.is_file() {
                return dir.parent().map(|v| v.into());
            }
            Some(dir)
        });

    let mut config = Config::default();
    let mut path = None;
    // Load the config filee path if provided
    if let Some(config_file) = config_path {
        let (cfg_tmp, path_tmp) = resolve_config(config_file.as_ref())
            .expect(&format!("Error resolving config for {:?}", config_file));
        config = cfg_tmp;
        path = path_tmp;
    };
    if let Some(path) = path.as_ref() {
        println!("Using rustfmt config file {}", path.display());
    }

    config.skip_children = skip_children;
    config.verbose = false;
    // write mode is always Plain for anaconda_rust buffer mode
    config.write_mode = WriteMode::Plain;

    // run the command and return status code
    process_summary(run(Input::Text(buffer), &config))
}

fn process_summary(error_summary: Summary) -> i32 {
    let status_code: i32;
    if error_summary.has_operational_errors() {
        status_code =1
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

fn usage(opts: &Options, reason: &str) {
    let reason = format!("{}\nusage: {} [options] <file>...",
                         reason,
                         env::args_os().next().unwrap().to_string_lossy());
    println!("{}", opts.usage(&reason));
}

#[no_mangle]
pub extern fn get_version() -> *const c_char {
    let s = CString::new(option_env!("CARGO_PKG_VERSION").unwrap_or("unknown")).unwrap();
    s.as_ptr()
}

#[no_mangle]
pub extern fn format(code: *const c_char, path: *const c_char) ->  c_uchar {
    let rcode = c_str_to_safe_string(code);
    let rpath = c_str_to_safe_string(path);

    println!("{}", rcode);
    println!("{}", rpath);
    1
}
