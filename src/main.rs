use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, watch};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use std::net::SocketAddr;
use tokio_tungstenite::{
    accept_async,
    tungstenite::Error,
};
use std::collections::HashMap;
use rand::{thread_rng, Rng};

const PI: f32 = 3.14159265358979323846;
const MAP_SIZE: usize = 20;
const MAP_SIZE_F32: f32 = MAP_SIZE as f32;

#[derive(Clone, Serialize)]
struct Bullet {
    ship_id: usize,
    x: f32,
    y: f32,
    angle: f32,
    v: f32,
    ttl: f32,
    hp: f32,
}

#[derive(Clone, Serialize)]
struct Ship {
    x: f32,
    y: f32,
    current_angle: f32,
    hp: f32,
    angle: f32,
    shape: i32,
    v: f32,
    max_hp: f32,
    turn_rate: f32,
    repair_rate: f32,
    bullet_ttl: f32,
    bullet_speed: f32,
    bullet_hp: f32,
}

// {"AddBullet":{"id":0,"angle":0.0}}
#[derive(Deserialize, Debug)]
enum Action {
    MoveShip {angle: f32 },
    AddBullet {angle: f32 },
}

struct ShipAction {
    ship_id: usize,
    action: Action,
}

impl ShipAction {
    fn new(ship_id: usize, action: Action) -> ShipAction {
        ShipAction {
            ship_id,
            action,
        }
    }
}

impl Bullet {
    fn new(ship_id: usize, ship: &Ship, angle: f32) -> Bullet {
        Bullet {
            ship_id: ship_id,
            x: ((ship.x + 1.0 * angle.cos()) + MAP_SIZE_F32) % MAP_SIZE_F32,
            y: ((ship.y + 1.0 * angle.sin()) + MAP_SIZE_F32) % MAP_SIZE_F32,
            angle,
            v: ship.bullet_speed,
            ttl: ship.bullet_ttl,
            hp: ship.bullet_hp,
        }
    }
}

impl Ship {
    fn new(x: f32, y: f32, angle: f32) -> Ship {
        Ship {
            x,
            y,
            angle: angle,
            current_angle: angle,
            shape: 0,
            turn_rate: 0.5,
            v: 0.2,
            repair_rate: 0.01,
            max_hp: 100.0,
            hp: 100.0,
            bullet_ttl: 1.0,
            bullet_speed: 0.2,
            bullet_hp: 10.0,
        }
    }

    fn apply_enchance(&mut self, enchance: Enchance) {
        self.turn_rate += enchance.turn_rate;
        self.v += enchance.v;
        self.repair_rate += enchance.repair_rate;
        self.max_hp += enchance.max_hp;
        self.bullet_ttl += enchance.bullet_ttl;
        self.bullet_speed += enchance.bullet_speed;
        self.bullet_hp += enchance.bullet_hp;
    }
}

struct Enchance {
    turn_rate: f32,
    v: f32,
    repair_rate: f32,
    max_hp: f32,
    bullet_ttl: f32,
    bullet_speed: f32,
    bullet_hp: f32,
}

impl Enchance {
    fn new(ship: &Ship) -> Enchance {
        let mut rng = thread_rng();
        Enchance {
            turn_rate: ship.turn_rate * rng.gen::<f32>(),
            v: ship.v * rng.gen::<f32>(),
            repair_rate: ship.repair_rate * rng.gen::<f32>(),
            max_hp: ship.max_hp * rng.gen::<f32>(),
            bullet_ttl: ship.bullet_ttl * rng.gen::<f32>() * 0.5,
            bullet_speed: ship.bullet_speed * rng.gen::<f32>() * 0.5,
            bullet_hp: ship.bullet_hp * rng.gen::<f32>(),
        }
    }
}

type Map<T, const N: usize> = [[T; N]; N];

type CurrentMap = Map<i32, MAP_SIZE>;


#[derive(Serialize, Clone)]
struct State(usize, HashMap<usize, Ship>, HashMap<usize, Bullet>, CurrentMap, Vec<(usize, usize)>);

async fn run(mut action_receiver: mpsc::Receiver<ShipAction>, map_sender: watch::Sender<State>) {
    let mut ships = HashMap::<usize, Ship>::new();
    let mut bullets = HashMap::<usize, Bullet>::new();
    let mut damage_feed = Vec::<(usize, usize, f32)>::new();
    // > 0 - ship id; < 0 - bullet
    let tick_rate = 0.1;
    let tick_rate_dur = tokio::time::Duration::from_secs_f32(tick_rate);
    let mut bullet_id = 0usize;
    'main: loop {
        let start_time = tokio::time::Instant::now();
        damage_feed.clear();
        let mut new_map = CurrentMap::default();
        bullets.retain(|b_id, b| {
            if b.ttl <= 0.0 || b.hp <= 0.0 {
                return false
            }
            b.x = ((b.x + b.v * b.angle.cos()) + MAP_SIZE_F32) % MAP_SIZE_F32;
            b.y = ((b.y + b.v * b.angle.sin()) + MAP_SIZE_F32) % MAP_SIZE_F32;
            new_map[b.y.round() as usize][b.x.round() as usize] = -(*b_id as i32);
            b.ttl -= tick_rate;
            true
        });
        ships.retain(|ship_id,  s| {
            if s.hp <= 0.0 {
                return false;
            }
            if s.hp < s.max_hp {
                s.hp += s.repair_rate;
            }
            let (nx, ny) = if s.current_angle == s.angle {
                let nx = ((s.x + s.v * s.angle.cos()) + MAP_SIZE_F32) % MAP_SIZE_F32;
                let ny = ((s.y + s.v * s.angle.sin()) + MAP_SIZE_F32) % MAP_SIZE_F32;
                (nx, ny)
            } else {
                let remain = s.angle - s.current_angle;
                if remain.abs() < s.turn_rate {
                    s.current_angle = s.angle;
                } else {
                    s.current_angle += s.turn_rate * remain.signum();
                }
                (s.x, s.y)
            };
            let rnx = nx.round() as usize;
            let rny = ny.round() as usize;
            match new_map[rny][rnx] {
                c if c > 0 => {
                    // Collision
                }
                c if c == 0 => { // Nothing
                    new_map[rny][rnx] = *ship_id as i32;
                    s.x = nx;
                    s.y = ny;
                }
                bid => { // Bullet
                    let bullet = bullets.get_mut(&(bid.abs() as usize)).expect("Bullet on map");
                    let bullet_hp = bullet.hp;
                    bullet.hp -= s.hp;
                    s.hp -= bullet_hp;
                    damage_feed.push((bullet.ship_id, *ship_id, s.hp));
                }
            }
            true
        });
        for (damager, damaged, remain_hp) in damage_feed.iter() {
            if *remain_hp <= 0.0 {
                if let Some(damager) = ships.get_mut(&damager) {
                    if let Some(damaged) = ships.get_mut(&damaged) {
                        damager.apply_enchance(Enchance::new(damaged));
                    }
                }
            }
        }
        loop {
            match action_receiver.try_recv() {
                Err(e) => {
                    match e {
                        TryRecvError::Empty => { break }
                        TryRecvError::Disconnected => { break 'main }
                    }
                }
                Ok(action) => {
                    let ShipAction { ship_id: id, action } = action;
                    match action {
                        Action::MoveShip {angle} => {
                            ships.entry(id as usize).and_modify(|sh| {
                                sh.angle = angle;
                            }).or_insert_with(|| {
                                let mut rng = thread_rng();
                                let angle = rng.gen_range(0.0..2.0 * PI);
                                let mut sx = rng.gen_range(0..MAP_SIZE);
                                let mut sy = rng.gen_range(0..MAP_SIZE);
                                while new_map[sy][sx] != 0 {
                                    sx = rng.gen_range(0..MAP_SIZE);
                                    sy = rng.gen_range(0..MAP_SIZE);
                                }
                                Ship::new(sx as f32, sy as f32, angle)
                            });
                        }
                        Action::AddBullet {angle} => {
                            if let Some(ship) = ships.get(&(id as usize)) {
                                bullets.insert(bullet_id, Bullet::new(id, ship, angle));
                                bullet_id += 1;
                            }
                        }
                    }
                }
            }

        }
        map_sender.send(State(0, ships.clone(), bullets.clone(), new_map, damage_feed.clone())).expect("map send");
        tokio::time::sleep(tick_rate_dur - start_time.elapsed()).await;
    }
}

async fn ws_sender_f(peer_id: usize, mut sink: SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>, mut map_receiver: watch::Receiver<State>) {
    while map_receiver.changed().await.is_ok() {
        let mut val = map_receiver.borrow_and_update().clone();
        val.0 = peer_id;
        let json = serde_json::to_string(&val).expect("json serialization");
        if let Err(_) = sink.send(Message::text(json)).await {
            break
        }
    }
}

async fn ws_receiver_f(peer_id: usize, mut stream: SplitStream<WebSocketStream<TcpStream>>, action_sender: mpsc::Sender<ShipAction>) {
    loop {
        if let Some(msg) = stream.next().await {
            if let Ok(msg) = msg {
                if let Message::Text(text) = msg {
                    if let Ok(action) = serde_json::from_str::<Action>(&text) {
                        if let Err(_) = action_sender.send(ShipAction::new(peer_id, action)).await {
                            break
                        }
                    }
                }
            } else {
                break
            }
        } else {
            break;
        }
    }
}

async fn accept_connection(stream: TcpStream, peer_id: usize, action_sender: mpsc::Sender<ShipAction>, map_receiver: watch::Receiver<State>) {
    match accept_async(stream).await {
        Ok(ws_stream) => {
            let (ws_sender, ws_receiver) = ws_stream.split();
            tokio::spawn(ws_receiver_f(peer_id, ws_receiver, action_sender));
            tokio::spawn(ws_sender_f(peer_id, ws_sender, map_receiver));
        }
        Err(e) => {
            match e {
                Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
                err => eprintln!("Error processing connection: {}", err),
            }

        }
    }
}

#[tokio::main]
async fn main() {
    let (action_sender, action_receiver) = mpsc::channel::<ShipAction>(32);
    let (map_sender, map_receiver) = watch::channel::<State>(State(0, HashMap::new(), HashMap::new(), CurrentMap::default(), vec![]));
    // let (map_sender, mut map_receiver) = watch::channel::<State>(State(vec![], vec![], [[0i8; 20]; 20]));
    let ws_addr = SocketAddr::from(([0, 0, 0, 0], 48666));
    let listener = TcpListener::bind(ws_addr).await.expect("Failed to bind");

    println!("Listening on: {}", ws_addr);

    tokio::spawn(async move {
        run(action_receiver, map_sender).await;
    });

    let mut id = 1usize;
    while let Ok((stream, _)) = listener.accept().await {
        if let Ok(addr) = stream.peer_addr() {
            println!("New connection: {id} - {addr}");
            tokio::spawn(accept_connection(stream, id, action_sender.clone(), map_receiver.clone()));
            id += 1;
        }
    }
}
