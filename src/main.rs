use itertools::Itertools;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{self, BufWriter, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

use tokio::sync::broadcast;
use tungstenite::{accept, Message};

use rand::prelude::SliceRandom;
#[tokio::main]
async fn main() {
    let names: Value = serde_json::from_str(include_str!("../names.json")).unwrap();
    let names: Vec<&str> = names["names"]
        .as_array()
        .expect("The names field should be an array. It is either missing or not an array")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let history: Value = serde_json::from_str(include_str!("../history.json")).unwrap();
    let mut history =
        history["history"]
            .as_array()
            .expect("The history field should be an array. It is either missing or not an array")
            .iter()
            .map(|arr| {
                arr.as_array()
                .expect(
                    format!("Each element within history should be an array. One of them isn't an array: {arr:?}").as_str(),
                )
                .iter()
                .map(|value| {
                    (*value).as_u64().expect(format!(
                        "A subelement of an array of history is not a u64: {}",
                        *value
                    ).as_str()) as usize
                })
                .inspect(|v| {
                    if *v >= 16 {
                        panic!("Values should be in the range [0,16)")
                    }
                })
                .collect::<Vec<usize>>()
            }).rev()
            .collect::<Vec<Vec<usize>>>();
    for a in &history {
        println!("{a:?}")
    }

    let (tx, _rx) = broadcast::channel::<Vec<String>>(1024);
    let transmitter = Arc::new(Mutex::new(tx.clone()));
    let current = Arc::new(Mutex::new(mapnames(&fliplast(&history[0]), &names)));
    let current2 = Arc::clone(&current);
    let server = TcpListener::bind("0.0.0.0:9003").unwrap();
    tokio::spawn(async move {
        for stream in server.incoming() {
            println!("Client connected!");
            let transmitter = Arc::clone(&transmitter);
            let current = Arc::clone(&current2);
            let tx = transmitter.lock().unwrap();
            let mut rx = tx.subscribe();
            let stream = stream.unwrap();
            let ip = stream.peer_addr().unwrap();
            let mut websocket = match accept(stream) {
                Ok(x) => x,
                Err(x) => {
                    println!("Handshakerror {x:?}");
                    continue;
                }
            };
            let default = current.lock().unwrap().clone();
            println!("Sendt {} to {ip:?}", default.join(","));
            websocket.send(Message::from(default.join(","))).unwrap();
            {
                thread::spawn(move || {
                    loop {
                        let val = rx.blocking_recv().unwrap();
                        let tosend = val
                            .iter()
                            .map(|x| x.to_string())
                            .collect::<Vec<String>>()
                            .join(",");
                        println!("Sendt {tosend} to {ip:?}");
                        match websocket.send(Message::from(tosend)) {
                            Ok(_) => (),
                            Err(_) => break,
                        }
                    }
                    println!("Client has disconnected");
                });
            }
        }
    });

    let mut list = history[history.len() - 1].clone();
    let mut neighbours: HashMap<usize, HashSet<usize>> = generate_neighbours(&history);
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let split_input = input.split_whitespace().collect::<Vec<&str>>();
        match split_input.iter().nth(0) {
            Some(&"write") => {
                if *history.last().expect("history has 0 length") == list {
                    println!("Already pushed");
                } else {
                    println!("pushing");
                    history.push(list.clone());
                    neighbours = generate_neighbours(&history);
                    // write_history(&json!({"history": history}));
                }
            }
            _ => {
                let iters = split_input
                    .iter()
                    .nth(0)
                    .unwrap_or(&"1")
                    .parse::<u64>()
                    .unwrap_or(1);
                let sleep_time = split_input
                    .iter()
                    .nth(1)
                    .unwrap_or(&"500")
                    .parse::<u64>()
                    .unwrap_or(500);
                let previous_seating = &history[history.len() - 2];
                let current_seating = &history[history.len() - 1];

                for iter in 0..iters {
                    if iter > 1 {
                        thread::sleep(std::time::Duration::from_millis(sleep_time));
                    }
                    let mut i = 0;
                    loop {
                        list.shuffle(&mut rand::thread_rng());
                        if !list.iter().zip(previous_seating).any(|(a, b)| *a == *b)
                            && !list.iter().zip(current_seating).any(|(&a, &b)| {
                                section(current_seating.iter().position(|x| *x == a).unwrap())
                                    == section(current_seating.iter().position(|x| *x == b).unwrap())
                            })
                            && !list
                                .iter()
                                .tuple_windows()
                                .any(|(a, b)| neighbours.get(&a).unwrap().contains(&b))
                        {
                            break;
                        }
                        i += 1;
                    }
                    println!("{i}");
                    let names = mapnames(&fliplast(&list), &names);
                    *current.lock().unwrap() = names.clone();
                    tx.send(names).unwrap();
                }
                println!("done");
            }
        }
    }
}

fn section(value: usize) -> usize {
    vec![0, 0, 1, 1, 2, 2, 3, 3, 3, 3, 2, 2, 1, 1, 0, 0][value]
}

fn fliplast(list: &Vec<usize>) -> Vec<usize> {
    list.iter()
        .take(9)
        .chain(list.iter().skip(9).rev())
        .map(|x| *x)
        .collect::<Vec<usize>>()
}

fn write_history(to_write: &Value) {
    let mut writer = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .open("history.json")
            .unwrap(),
    );
    serde_json::to_writer_pretty(&mut writer, &to_write).unwrap();
    writer.flush().unwrap();
}

fn mapnames(indicies: &Vec<usize>, names: &Vec<&str>) -> Vec<String> {
    let mut ret = Vec::new();
    for &i in indicies {
        ret.push(names[i].to_string());
    }
    ret
}

fn generate_neighbours(history: &Vec<Vec<usize>>) -> HashMap<usize, HashSet<usize>> {
    let mut neighbours: HashMap<usize, HashSet<usize>> = HashMap::new();
    neighbours.insert(
        history[history.len() - 2][0],
        HashSet::from([(history[history.len() - 2][1])]),
    );
    neighbours.insert(
        history[history.len() - 2][history[history.len() - 2].len() - 1],
        HashSet::from([(history[history.len() - 2][history[history.len() - 2].len() - 2])]),
    );
    for seat in history[history.len() - 2].windows(3) {
        neighbours.insert(seat[1], HashSet::from([(seat[0]), (seat[2])]));
    }
    // for neigbour in &neighbours {
    //     print!("({}: ", *neigbour.0);
    //     for v in neigbour.1 {
    //         print!(" {}", *v);
    //     }
    //     print!("), ");
    //     println!();
    // }
    neighbours
}
