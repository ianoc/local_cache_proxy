extern crate clap;
extern crate futures;
extern crate hyper;
extern crate hyper_proxy;
extern crate local_cache_proxy;
extern crate lru_disk_cache;
extern crate pretty_env_logger;
extern crate protobuf;
extern crate rusoto_core;
extern crate rusoto_s3;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_uds;

use clap::{App, Arg};
use futures::Future;
use futures::Stream;
use rusoto_s3::GetObjectRequest;
use rusoto_s3::PutObjectRequest;
use std::io::Read;
use std::io::Write;

use rusoto_core::Region;

use rusoto_s3::{S3, S3Client};

#[macro_use]
extern crate log;

// use std::{thread, time};

// use protobuf::error::ProtobufError;
// use protobuf::{CodedInputStream, Message}; //, ProtobufResult, RepeatedField};
use std::fs::File;
// use std::io::BufReader;
// use std::io::{self, stdin, BufRead, BufReader};
// use local_cache_proxy::action_result::ActionResult;
use std::path::Path;

fn put_object_with_file_name(
    client: &S3Client,
    bucket: &str,
    dest_filename: &str,
    local_filename: &Path,
) {
    let mut f = File::open(local_filename).unwrap();
    let mut contents: Vec<u8> = Vec::new();
    match f.read_to_end(&mut contents) {
        Err(why) => panic!("Error opening file to send to S3: {}", why),
        Ok(_) => {
            let req = PutObjectRequest {
                bucket: bucket.to_owned(),
                key: dest_filename.to_owned(),
                body: Some(contents.into()),
                ..Default::default()
            };
            let result = client.put_object(&req).sync().expect("Couldn't PUT object");
            println!("{:#?}", result);
        }
    }
}

fn get_object_with_file_name(client: &S3Client, bucket: &str, prefix: &str, local_filename: &Path) {
    let get_req = GetObjectRequest {
        bucket: bucket.to_owned(),
        key: prefix.to_owned(),
        ..Default::default()
    };

    let mut f = File::create(local_filename).unwrap();

    let result = client
        .get_object(&get_req)
        .sync()
        .expect("Couldn't GET object");
    println!("get object result: {:#?}", result);

    let stream = result.body.unwrap();
    stream
        .for_each(|e| {
            f.write_all(&e).unwrap();
            Ok(())
        })
        .wait()
        .unwrap();
}

fn main() {
    pretty_env_logger::init();

    let client = S3Client::simple(Region::UsWest2);

    let matches = App::new("Read a local protobuf file and show debug info.")
        .version("0.1")
        .about("Testing using hte protobuf apis")
        .author("Ian O Connell <ianoc@ianoc.net>")
        .arg(
            Arg::with_name("bucket")
                .short("b")
                .long("bucket")
                .value_name("BUCKET")
                .required(true)
                .help("Use the file path to read")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("prefix")
                .short("p")
                .long("prefix")
                .value_name("PREFIX")
                .required(true)
                .help("Use the file path to read")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("file")
                .short("f")
                .long("file")
                .value_name("FILE_PATH")
                .required(true)
                .help("Use the file path to read")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("upload")
                .long("upload")
                .required(false)
                .help("should do the upload path")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("download")
                .long("download")
                .required(false)
                .help("should do the download path")
                .takes_value(false),
        )
        .get_matches();

    let local_file = Path::new(matches.value_of("file").expect("Require a file path"));
    let bucket = matches.value_of("bucket").expect("Require a bucket");
    let prefix = matches.value_of("prefix").expect("Require a prefix");

    info!("{:?}", matches.value_of("download"));

    match (matches.is_present("upload"), matches.is_present("download")) {
        (true, false) => put_object_with_file_name(&client, bucket, prefix, local_file),
        (false, true) => get_object_with_file_name(&client, bucket, prefix, local_file),
        (true, true) => panic!("Both upload and download specified"),
        (false, false) => panic!("One of upload or download must be specified"),
    };
    // let mut s = ActionResult::new();

    // info!("Reading file {:?}", path);

    // if path.exists() {
    //     let file = File::open(&path).map_err(ProtobufError::IoError).unwrap();
    //     let mut br = BufReader::new(file);
    //     let mut cis = CodedInputStream::from_buffered_reader(&mut br);
    //     s.merge_from(&mut cis).unwrap();
    // } else {
    //     panic!("Path {:?} does not exist!", path);
    // }

    // let blacklist = ["external/io_bazel_rules_go"];

    // let mut blacklisted = false;
    // for i in s.output_directories.iter() {
    //     println!("{:?}", i);
    //     for e in blacklist.iter() {
    //         if i.path.contains(e) {
    //             blacklisted = true;
    //         }
    //     }
    // }

    // warn!("Blacklisted? : {:?}", blacklisted);

    // info!("Contents of s: {:?}", s.output_directories);
    // info!("Contents of s: {:?}", s);

    // info!("Contents of s: {:?}", s.fields);
}
