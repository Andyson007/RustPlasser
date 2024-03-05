use itertools::Itertools;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::{self, BufWriter, Write},
    net::TcpListener,
    ops::{Index, IndexMut},
    slice::SliceIndex,
    sync::{Arc, Mutex},
    thread,
};

use tokio::sync::broadcast;
use tungstenite::{accept, Message};

use rand::{prelude::SliceRandom, random};

enum Input {
    Write(bool),
    Scramble { iters: usize, sleep: u64 },
    Reset,
}

impl Input {
    pub fn read() -> Option<Self> {
        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .expect("Error while reading line");
        let line = line.lines().nth(0).unwrap();
        let split = line.split_whitespace().collect::<Vec<&str>>();
        if split.is_empty() {
            return Some(Input::Scramble {
                iters: 1,
                sleep: 500,
            });
        }
        match split[0] {
            "write" => {
                if split.len() > 1 {
                    Some(Input::Write(split[1] == "json"))
                } else {
                    Some(Input::Write(false))
                }
            }
            "reset" => Some(Input::Reset),
            _ => {
                if !split.iter().all(|x| x.chars().all(|x| x.is_ascii_digit())) {
                    return None;
                }
                match split.len() {
                    0 => unreachable!(),
                    1 => Some(Input::Scramble {
                        iters: split[0].parse::<usize>().unwrap(),
                        sleep: 500,
                    }),
                    2.. => Some(Input::Scramble {
                        iters: split[0].parse::<usize>().unwrap(),
                        sleep: split[1].parse::<u64>().unwrap(),
                    }),
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let names: Value = serde_json::from_str(include_str!("../names.json")).unwrap();
    let names: Vec<&str> = names["names"]
        .as_array()
        .expect("The names field should be an array. It is either missing or not an array")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let history: Value = serde_json::from_str(include_str!("../history.json"))
        .expect("Couldn't find file history.json");
    let mut history =
        history["history"]
            .as_array()
            .expect("The history field should be an array. It is either missing or not an array")
            .iter()
            .map(|arr| {
                arr.as_array()
                .unwrap_or_else(||
                    panic!("Each element within history should be an array. One of them isn't an array: {arr:?}"),
                )
                .iter()
                .map(|value| {
                    value.as_u64().unwrap_or_else(||panic!(
                        "A subelement of an array of history is not a u64: {}",
                        *value
                    )) as usize
                })
                .inspect(|v| {
                    if *v >= 16 {
                        panic!("Values should be in the range [0,16)")
                    }
                })
                .collect::<Vec<usize>>()
            })
            .collect::<Vec<Vec<usize>>>();

    for a in &history {
        println!("{a:?}")
    }

    let (tx, _rx) = broadcast::channel::<Vec<String>>(1024);
    let transmitter = Arc::new(Mutex::new(tx.clone()));
    let current = Arc::new(Mutex::new(mapnames(
        &fliplast(history.last().unwrap()),
        &names,
    )));
    let current2 = Arc::clone(&current);
    let server = TcpListener::bind("0.0.0.0:9003").unwrap();
    tokio::spawn(async { serve_server(server, current2, transmitter) });

    let mut list = history[history.len() - 1].clone();
    let mut neighbours: HashMap<usize, HashSet<usize>> =
        generate_neighbours(&history[history.len() - 1]);
    loop {
        let previous_seating = &history[history.len() - 2];
        let current_seating = &history[history.len() - 1];

        let input = loop {
            if let Some(x) = Input::read() {
                break x;
            }
            println!("Invalid input");
        };
        match input {
            Input::Write(x) => {
                if *current_seating == list {
                    println!("Already pushed");
                } else {
                    println!("pushing");
                    history.push(list.clone());
                    neighbours = generate_neighbours(&list);
                    println!("{history:?}")
                }
                if x {
                    let to_write = json!({"history": history});
                    let mut writer = BufWriter::new(
                        OpenOptions::new()
                            .write(true)
                            .truncate(true)
                            .open("history.json")
                            .unwrap(),
                    );
                    serde_json::to_writer_pretty(&mut writer, &to_write).unwrap();
                    writer.flush().unwrap();
                    println!("Wrote to json");
                }
            }
            Input::Reset => {
                let names = mapnames(&fliplast(current_seating), &names);
                *current.lock().unwrap() = names.clone();
                tx.send(names).unwrap();
            }
            Input::Scramble { iters, sleep } => {
                for iter in 0..iters {
                    if iter > 1 {
                        thread::sleep(std::time::Duration::from_millis(sleep));
                    }
                    generate_seating(
                        &mut list,
                        &[&previous_seating, &current_seating],
                        &neighbours,
                    );
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

fn fliplast(list: &[usize]) -> Vec<usize> {
    let amount = 8;
    list.iter()
        .take(amount)
        .chain(list.iter().skip(amount).rev())
        .copied()
        .collect::<Vec<usize>>()
}

fn mapnames(indicies: &[usize], names: &[&str]) -> Vec<String> {
    let mut ret = Vec::new();
    for &i in indicies {
        ret.push(names[i].to_string());
    }
    ret.insert(2, "".to_string());
    ret.insert(8, "".to_string());
    ret
}

fn generate_neighbours(seating: &[usize]) -> HashMap<usize, HashSet<usize>> {
    let mut neighbours: HashMap<usize, HashSet<usize>> = HashMap::new();
    neighbours.insert(seating[0], HashSet::from([seating[1]]));
    neighbours.insert(
        seating[seating.len() - 1],
        HashSet::from([(seating[seating.len() - 2])]),
    );
    for seat in seating.windows(3) {
        neighbours.insert(seat[1], HashSet::from([(seat[0]), (seat[2])]));
    }
    neighbours
}

fn serve_server(
    server: TcpListener,
    current: Arc<Mutex<Vec<String>>>,
    transmitter: Arc<Mutex<broadcast::Sender<Vec<String>>>>,
) {
    for stream in server.incoming() {
        println!("Client connected!");
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
                    // println!("Sendt {tosend} to {ip:?}");
                    match websocket.send(Message::from(tosend)) {
                        Ok(_) => (),
                        Err(_) => break,
                    }
                }
                println!("Client has disconnected");
            });
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct HeatMap {
    heatmap: [f64; 16],
}

impl<Idx> IndexMut<Idx> for HeatMap
where
    Idx: SliceIndex<[f64], Output = f64>,
{
    fn index_mut(&mut self, index: Idx) -> &mut Self::Output {
        &mut self.heatmap[index]
    }
}

impl<Idx> Index<Idx> for HeatMap
where
    Idx: SliceIndex<[f64], Output = f64>,
{
    type Output = f64;

    fn index(&self, index: Idx) -> &Self::Output {
        &self.heatmap[index]
    }
}

impl Default for HeatMap {
    fn default() -> Self {
        HeatMap {
            heatmap: [1.0f64; 16],
        }
    }
}

fn generate_seating(
    list: &mut Vec<usize>,
    seating: &[&Vec<usize>; 2],
    neighbours: &HashMap<usize, HashSet<usize>>,
) {
    let (previous_seating, current_seating) = (seating[0], seating[1]);
    let mut people = [HeatMap::default(); 16];

    for i in previous_seating.iter().enumerate() {
        // Set the heatmap for the current person in the current seat
        people[*i.1][i.0] = 0.0;
    }

    for i in current_seating.iter().enumerate() {
        // Set the heatmap for the current person in the current seat
        // people[*i.1][i.0] = 0.0;
        let sectionnum = section(i.0);
        for (spot, weight) in people[*i.1].heatmap.iter_mut().enumerate() {
            if section(spot) == sectionnum {
                *weight = 0.0f64;
            }
        }
    }

    let ans = generate_seating_from_map(&people, Vec::new());
    println!("{ans:?}");

    let mut i = 0;
    loop {
        i += 1;
        *list = generate_seating_from_map(&people, Vec::new()).unwrap();
        // list.shuffle(&mut rand::thread_rng());
        if !list
            .iter()
            .tuple_windows()
            .any(|(a, b)| neighbours.get(a).unwrap().contains(b))
        {
            break;
        }
    }
    println!("{i}");
}

fn generate_seating_from_map(
    people: &[HeatMap; 16],
    mut current: Vec<usize>,
) -> Option<Vec<usize>> {
    if current.len() == 16 {
        return Some(current);
    }
    let mut blacklist = HashSet::new();

    let iters = 16 - current.len() - blacklist.len();
    for _ in 0..iters {
        let mut iter = people
            .iter()
            .enumerate()
            .filter(|x| !current.contains(&x.0) && !blacklist.contains(&x.0));

        let sum = iter
            .clone()
            .map(|x| x.1)
            .map(|x| x[current.len()])
            .sum::<f64>();
        let rand = random::<f64>() * sum;

        let mut curr = 0f64;
        let mut ans = 0;

        while curr < rand {
            let val = iter.next()?;
            curr += val.1[current.len()];
            ans = val.0;
        }

        current.push(ans);
        if let Some(x) = generate_seating_from_map(people, current.clone()) {
            return Some(x);
        }
        blacklist.insert(ans);
    }
    None
}
