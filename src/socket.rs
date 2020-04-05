// socket.rs

// main.rs

use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct Input {
    pub sync: bool,
    pub weather: String,
    pub temperature: String,
}

impl Input {
    fn new() -> Input {
        Input {
            sync: false,
            weather: "----".to_string(),     //String::new(),
            temperature: "----".to_string(), //String::new(),
        }
    }
}

fn handle_client(stream: UnixStream, input: Arc<Mutex<Input>>, debug: bool) {
    if debug {
        println!("thread starting…");
    }

    let mut stream = BufReader::new(stream);
    loop {
        let mut buf = String::new();
        if stream.read_line(&mut buf).is_err() {
            break;
        }
        if buf.len() < 3 || buf == "\r\n" || buf == "\n" {
            if debug {
                println!("empty line");
            }
            break;
        }

        let set = {
            let mut f = input.lock().unwrap();
            let b = buf.trim();

            let command = &b[0..2];
            match command {
                "s=" => match &b[2..3] {
                    "1" | "y" | "Y" => {
                        (*f).sync = true;
                        true
                    }
                    "0" | "n" | "N" => {
                        (*f).sync = false;
                        true
                    }
                    _ => false,
                },
                "w=" => {
                    (*f).weather = b[2..].to_string();
                    true
                }
                "t=" => {
                    (*f).temperature = b[2..].to_string();
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

pub fn setup(socket: &str, debug: bool) -> std::io::Result<Arc<Mutex<Input>>> {
    match fs::remove_file(socket) {
        Ok(_) => (),
        Err(e) => {
            if e.kind() != ErrorKind::NotFound {
                return Err(e);
            }
        }
    }

    let listener = UnixListener::bind(socket)?;

    let flag = Arc::new(Mutex::new(Input::new()));

    let f = flag.clone();

    thread::spawn(move ||

                  // accept connections and process them, spawning a new thread for each one
                  for stream in listener.incoming() {
                      match stream {
                          Ok(stream) => {
                              let f = flag.clone();
                              thread::spawn(move || handle_client(stream, f ,debug));
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
