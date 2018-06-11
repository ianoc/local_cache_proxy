extern crate clap;
extern crate futures;
extern crate hyper;
extern crate hyper_proxy;
extern crate local_cache_proxy;
extern crate lru_disk_cache;
extern crate pretty_env_logger;
extern crate protobuf;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_uds;

use clap::{App, Arg};
#[macro_use]
extern crate log;

// use std::{thread, time};

use protobuf::error::ProtobufError;
use protobuf::{CodedInputStream, Message}; //, ProtobufResult, RepeatedField};
use std::fs::File;
use std::io::BufReader;
// use std::io::{self, stdin, BufRead, BufReader};
use local_cache_proxy::action_result::ActionResult;
use std::path::Path;

fn main() {
    pretty_env_logger::init();

    let matches = App::new("Read a local protobuf file and show debug info.")
        .version("0.1")
        .about("Testing using hte protobuf apis")
        .author("Ian O Connell <ianoc@ianoc.net>")
        .arg(
            Arg::with_name("file")
                .short("f")
                .long("file")
                .value_name("FILE_PATH")
                .required(true)
                .help("Use the file path to read")
                .takes_value(true),
        )
        .get_matches();

    let path = Path::new(matches.value_of("file").expect("Require a file path"));

    let mut s = ActionResult::new();

    info!("Reading file {:?}", path);

    if path.exists() {
        let file = File::open(&path).map_err(ProtobufError::IoError).unwrap();
        let mut br = BufReader::new(file);
        let mut cis = CodedInputStream::from_buffered_reader(&mut br);
        s.merge_from(&mut cis).unwrap();
    } else {
        panic!("Path {:?} does not exist!", path);
    }

    let blacklist = ["external/io_bazel_rules_go"];

    let mut blacklisted = false;
    for i in s.output_directories.iter() {
        println!("{:?}", i);
        for e in blacklist.iter() {
            if i.path.contains(e) {
                blacklisted = true;
            }
        }
    }

    warn!("Blacklisted? : {:?}", blacklisted);

    info!("Contents of s: {:?}", s.output_directories);

    // info!("Contents of s: {:?}", s.fields);
}
