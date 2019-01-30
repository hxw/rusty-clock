// socket.rs

// main.rs

use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
//use std::{thread, time};
use std::thread;

fn handle_client(stream: UnixStream, flag: Arc<Mutex<bool>>, debug: bool) {
    if debug {
        println!("thread starting…");
    }

    let mut stream = BufReader::new(stream);
    loop {
        let mut buf = String::new();
        if stream.read_line(&mut buf).is_err() {
            break;
        }
        if buf.len() < 1 || buf == "\r\n" || buf == "\n" {
            if debug {
                println!("empty line");
            }
            break;
        }

        let set = {
            let mut f = flag.lock().unwrap();

            match &buf.trim()[0..1] {
                "1" | "y" | "Y" => {
                    *f = true;
                    true
                }
                "0" | "n" | "N" => {
                    *f = false;
                    true
                }
                _ => false,
            }
        };
        if debug {
            if set {
                stream.get_ref().write("set to: ".as_bytes()).unwrap();
            }
            stream.get_ref().write(buf.as_bytes()).unwrap();
        }
    }
    if debug {
        println!("thread stopped");
    }
}

// fn watcher(flag: Arc<Mutex<bool>>) {
//     let mut old_flag = *flag.lock().unwrap();

//     loop {
//         let duration = time::Duration::from_millis(1500);

//         thread::sleep(duration);

//         let f = *flag.lock().unwrap();
//         if f != old_flag {
//             old_flag = f;

//             let now = time::Instant::now();
//             println!("flag changed to: {} at: {:?}", old_flag, now);
//         }
//     }
// }

pub fn setup(socket: &str, debug: bool) -> std::io::Result<Arc<Mutex<bool>>> {
    match fs::remove_file(socket) {
        Ok(_) => (),
        //        Err(Error::NotFound) => (),
        Err(e) => {
            if e.kind() != ErrorKind::NotFound {
                return Err(e);
            }
        }
    }

    let listener = UnixListener::bind(socket)?;

    let flag = Arc::new(Mutex::new(false));

    let f = flag.clone();
    //thread::spawn(move || watcher(f));
thread::spawn(move || 

    // accept connections and process them, spawning a new thread for each one
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let f = flag.clone();
                thread::spawn(move || handle_client(stream, f, debug));
            }
            Err(err) => {
                if debug {
                    println!("error: {}", err);
                }
                break;
            }
        }
    });

    Ok(f)
}
