use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{self, BufWriter, Write},
    net::TcpListener,
    sync::Arc,
    thread,
    time::Duration,
};

use tokio::sync::{broadcast, Mutex};
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

    let (tx, _rx) = broadcast::channel::<Vec<String>>(64);
    let transmitter = Arc::new(Mutex::new(tx.clone()));
    let current = Arc::new(Mutex::new(mapnames(&history[0], &names)));
    let current2 = Arc::clone(&current);
    let server = TcpListener::bind("0.0.0.0:9003").unwrap();
    tokio::spawn(async move {
        for stream in server.incoming() {
            println!("Client connected!");
            let transmitter = Arc::clone(&transmitter);
            let current = Arc::clone(&current2);
            let tx = transmitter.lock().await;
            let mut rx = tx.subscribe();
            let mut websocket = accept(stream.unwrap()).unwrap();
            let default = current.lock().await.clone();
            websocket.send(Message::from(default.join(","))).unwrap();
            println!("{default:?}");
            tokio::spawn(async move {
                println!("test");

                loop {
                    let val = rx.recv().await.unwrap();
                    match websocket.send(Message::from(
                        val.iter()
                            .map(|x| x.to_string())
                            .collect::<Vec<String>>()
                            .join(","),
                    )) {
                        Ok(_) => (),
                        Err(_) => break,
                    }
                }
                println!("Client has disconnected");
            })
            .await
            .unwrap();
        }
    });

    loop {
        io::stdin().read_line(&mut String::new()).unwrap();
        let mut neighbours: HashMap<usize, HashSet<usize>> = HashMap::new();
        neighbours.insert(history[history.len()-1][0], HashSet::from([(history[history.len()-1][1])]));
        neighbours.insert(
            history[history.len()-1][history[history.len()-1].len() - 1],
            HashSet::from([(history[history.len()-1][history[history.len()-1].len() - 2])]),
        );
        for seat in history[history.len()-1].windows(3) {
            neighbours.insert(seat[1], HashSet::from([(seat[0]), (seat[2])]));
        }
        let mut list = history[history.len()-1].clone();

        let mut i = 0;
        loop {
            list.shuffle(&mut rand::thread_rng());
            if !list.iter().zip(&history[history.len()-2]).any(|(a, b)| *a == *b)
                && !list
                    .iter()
                    .zip(&history[history.len()-1])
                    .any(|(&a, &b)| section(a) == section(b))
                && {
                    !list.windows(3).any(|arr| {
                        neighbours.get(&arr[1]).unwrap().contains(&arr[0])
                            || neighbours.get(&arr[1]).unwrap().contains(&arr[2])
                    }) && !neighbours.get(&list[0]).unwrap().contains(&list[1])
                        && !neighbours
                            .get(&list[list.len() - 1])
                            .unwrap()
                            .contains(&list[list.len() - 2])
                }
            {
                break;
            }
            i += 1;
        }
        println!("{i}");
        history.push(list.clone());
        let list = list
            .iter()
            .take(8)
            .chain(list.iter().skip(8).rev())
            .map(|x| *x)
            .collect::<Vec<usize>>();
        println!("{list:?}");
        let names = mapnames(&list, &names);
        println!("{:?}", names);
        *current.lock().await = names.clone();
        tx.send(names).unwrap();
        // write_history(&json!({"history": history}));
    }
}

fn section(value: usize) -> usize {
    vec![0, 0, 1, 1, 2, 2, 3, 3, 3, 3, 2, 2, 1, 1, 0, 0][value]
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
