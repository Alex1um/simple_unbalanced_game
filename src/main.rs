use tokio::sync::{mpsc, watch};

const MAP_SIZE: usize = 20;
const MAP_SIZE_F32: f32 = MAP_SIZE as f32;
const TICK_RATE: f32 = 1.0 / 60.0;
const MAX_UPGRADES: usize = 10;

mod state {
    use super::game::{Bullet, CurrentIDMap, CurrentTypeMap, Ship};
    use ahash::RandomState;
    use serde::Serialize;
    use std::collections::HashMap;

    #[derive(Serialize, Clone)]
    pub struct DamageFeedRecord(pub usize, pub usize, pub f32);

    pub type DamageFeed = Vec<DamageFeedRecord>;

    #[derive(Serialize, Clone, Default)]
    pub struct State(
        pub usize,
        pub HashMap<usize, Ship, RandomState>,
        pub HashMap<usize, Bullet, RandomState>,
        pub CurrentIDMap,
        pub CurrentTypeMap,
        pub DamageFeed,
    );
}

mod game {
    use crate::MAX_UPGRADES;

    use super::state::{DamageFeed, DamageFeedRecord, State};
    use super::{MAP_SIZE, MAP_SIZE_F32, TICK_RATE};
    use ahash::RandomState;
    use rand::{thread_rng, Rng};
    use serde::{Deserialize, Serialize};
    use serde_repr::Serialize_repr;
    use std::collections::HashMap;
    use std::f32::consts::TAU;
    use tokio::sync::mpsc::error::TryRecvError;
    use tokio::sync::{mpsc, watch};
    use mlua::prelude::*;

    #[derive(Clone, Serialize, Deserialize)]
    pub struct Bullet {
        ship_id: usize,
        x: f32,
        y: f32,
        angle: f32,
        v: f32,
        ttl: f32,
        hp: f32,
    }

    #[derive(Clone, Serialize, Deserialize, Debug)]
    pub struct Ship {
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
    pub enum Action {
        MoveShip { angle: f32 },
        AddBullet { angle: f32 },
    }

    pub struct ShipAction {
        ship_id: usize,
        action: Action,
    }

    impl ShipAction {
        pub fn new(ship_id: usize, action: Action) -> ShipAction {
            ShipAction { ship_id, action }
        }
    }

    impl Bullet {
        fn new(ship_id: usize, ship: &Ship, angle: f32) -> Bullet {
            Bullet {
                ship_id,
                x: ((ship.x + 1.0 * angle.cos()) + MAP_SIZE_F32) % MAP_SIZE_F32,
                y: ((ship.y + 1.0 * angle.sin()) + MAP_SIZE_F32) % MAP_SIZE_F32,
                angle,
                v: ship.bullet_speed + ship.v,
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
                angle,
                current_angle: angle,
                shape: 0,
                turn_rate: 0.5,
                v: 0.08,
                repair_rate: 0.01,
                max_hp: 100.0,
                hp: 100.0,
                bullet_ttl: 1.0,
                bullet_speed: 0.02,
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
                v: ship.v * rng.gen::<f32>() * 0.2,
                repair_rate: ship.repair_rate * rng.gen::<f32>(),
                max_hp: ship.max_hp * rng.gen::<f32>(),
                bullet_ttl: ship.bullet_ttl * rng.gen::<f32>() * 0.3,
                bullet_speed: ship.bullet_speed * rng.gen::<f32>() * 0.3,
                bullet_hp: ship.bullet_hp * rng.gen::<f32>(),
            }
        }

        fn random() -> Enchance {
            let mut rng = thread_rng();
            Enchance {
                turn_rate: rng.gen::<f32>() * 0.3,
                v: rng.gen::<f32>() * 0.2,
                repair_rate: rng.gen::<f32>(),
                max_hp: rng.gen::<f32>() * 20.0,
                bullet_ttl: rng.gen::<f32>() * 0.3,
                bullet_speed: rng.gen::<f32>() * 0.3,
                bullet_hp: rng.gen::<f32>(),
            }
        }
    }

    #[derive(Clone, Copy, Serialize_repr, PartialEq)]
    #[repr(i8)]
    pub enum ObjectType {
        None = 0,
        Ship = 1,
        Bullet = -1,
        Upgrade = 2,
    }

    impl Default for ObjectType {
        fn default() -> Self {
            ObjectType::None
        }
    }

    type Map<T, const N: usize> = [[T; N]; N];

    pub type CurrentIDMap = Map<usize, MAP_SIZE>;
    
    pub type CurrentTypeMap = Map<ObjectType, MAP_SIZE>;

    pub async fn run(
        mut action_receiver: mpsc::Receiver<ShipAction>,
        map_sender: watch::Sender<State>,
    ) {
        let lua = Lua::new();
        let globals = lua.globals();
        let script = lua.load(include_str!("scripts/main.lua")).into_function().unwrap();
        let mut ships = HashMap::<usize, Ship, RandomState>::default();
        let mut bullets = HashMap::<usize, Bullet, RandomState>::default();
        let mut upgrades = HashMap::<usize, Enchance, RandomState>::default();
        let mut damage_feed = DamageFeed::new();
        // > 0 - ship id; < 0 - bullet
        let mut bullet_id = 0usize;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs_f32(TICK_RATE));
        'main: loop {
            interval.tick().await;
            damage_feed.clear();
            let mut new_id_map = CurrentIDMap::default();
            let mut new_type_map = CurrentTypeMap::default();
            
            globals.set("ships", lua.to_value(&ships).unwrap()).unwrap();
            globals.set("bullets", lua.to_value(&bullets).unwrap()).unwrap();
            script.call::<_, ()>(()).unwrap();
            ships = lua.from_value(globals.get("ships").unwrap()).unwrap();
            bullets = lua.from_value(globals.get("bullets").unwrap()).unwrap();

            bullets.retain(|b_id, b| {
                if b.ttl <= 0.0 || b.hp <= 0.0 {
                    return false;
                }
                b.x = ((b.x + b.v * b.angle.cos()) + MAP_SIZE_F32) % MAP_SIZE_F32;
                b.y = ((b.y + b.v * b.angle.sin()) + MAP_SIZE_F32) % MAP_SIZE_F32;
                let cx = b.x.round() as usize % MAP_SIZE;
                let cy = b.y.round() as usize % MAP_SIZE;
                new_id_map[cy][cx] = *b_id;
                new_type_map[cy][cx] = ObjectType::Bullet;
                b.ttl -= TICK_RATE;
                true
            });
            ships.retain(|ship_id, s| {
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
                let rnx = nx.round() as usize % MAP_SIZE;
                let rny = ny.round() as usize % MAP_SIZE;
                match new_type_map[rny][rnx] {
                    ObjectType::Ship => {
                        // Collision
                    }
                    ObjectType::None => {
                        // Nothing
                        new_id_map[rny][rnx] = *ship_id;
                        new_type_map[rny][rnx] = ObjectType::Ship;
                        s.x = nx;
                        s.y = ny;
                    }
                    ObjectType::Upgrade => {
                        new_id_map[rny][rnx] = *ship_id;
                        new_type_map[rny][rnx] = ObjectType::Ship;
                        s.x = nx;
                        s.y = ny;
                        if let Some(upgrade) = upgrades.remove(&{ *ship_id }) {
                            s.apply_enchance(upgrade);
                        }
                    }
                    ObjectType::Bullet => {
                        // Bullet
                        let bid = new_id_map[rny][rnx];
                        new_id_map[rny][rnx] = *ship_id;
                        new_type_map[rny][rnx] = ObjectType::Ship;
                        s.x = nx;
                        s.y = ny;
                        let bullet = bullets
                            .get_mut(&(bid))
                            .expect("Bullet on map");
                        let bullet_hp = bullet.hp;
                        bullet.hp -= s.hp;
                        s.hp -= bullet_hp;
                        damage_feed.push(DamageFeedRecord(bullet.ship_id, *ship_id, s.hp));
                    }
                }
                true
            });
            loop { // receive actions
                match action_receiver.try_recv() {
                    Err(e) => match e {
                        TryRecvError::Empty => break,
                        TryRecvError::Disconnected => break 'main,
                    },
                    Ok(action) => {
                        let ShipAction {
                            ship_id: id,
                            action,
                        } = action;
                        match action {
                            Action::MoveShip { angle } => {
                                ships
                                    .entry(id)
                                    .and_modify(|sh| {
                                        sh.angle = angle;
                                    })
                                    .or_insert_with(|| {
                                        let mut rng = thread_rng();
                                        let angle = rng.gen_range(0.0..TAU);
                                        let mut sx = rng.gen_range(0..MAP_SIZE);
                                        let mut sy = rng.gen_range(0..MAP_SIZE);
                                        while new_type_map[sy][sx] != ObjectType::None {
                                            sx = rng.gen_range(0..MAP_SIZE);
                                            sy = rng.gen_range(0..MAP_SIZE);
                                        }
                                        Ship::new(sx as f32, sy as f32, angle)
                                    });
                            }
                            Action::AddBullet { angle } => {
                                if let Some(ship) = ships.get_mut(&{ id }) {
                                    ship.hp -= 1.0;
                                    damage_feed.push(DamageFeedRecord(id, id, ship.hp));
                                    bullets.insert(bullet_id, Bullet::new(id, ship, angle));
                                    bullet_id += 1;
                                }
                            }
                        }
                    }
                }
            }
            for DamageFeedRecord(damager, damaged, remain_hp) in damage_feed.iter() {
                if *remain_hp <= 0.0 {
                    if let Some(damaged) = ships.get(damaged) {
                        let enchance = Enchance::new(damaged);
                        if let Some(damager) = ships.get_mut(damager) {
                            damager.apply_enchance(enchance);
                        }
                    }
                }
            }

            // respawn upgrades. BUG: map overflow
            // for _ in 0..(MAX_UPGRADES - current_updates_count) {
            //     let mut rng = thread_rng();
            //     let mut sx = rng.gen_range(0..MAP_SIZE);
            //     let mut sy = rng.gen_range(0..MAP_SIZE);
            //     while new_type_map[sy][sx] != ObjectType::None {
            //         sx = rng.gen_range(0..MAP_SIZE);
            //         sy = rng.gen_range(0..MAP_SIZE);
            //     }
            //     new_type_map[sy][sx] = ObjectType::Upgrade;
            //     current_updates_count += 1;
            // }
            map_sender
                .send(State(
                    0,
                    ships.clone(),
                    bullets.clone(),
                    new_id_map,
                    new_type_map,
                    damage_feed.clone(),
                ))
                .expect("map send");
        }
    }
}

mod wserver {

    use super::game::{Action, ShipAction};
    use super::state::State;
    use futures::stream::{SplitSink, SplitStream};
    use futures::{SinkExt, StreamExt};
    use tokio::net::TcpStream;
    use tokio::sync::{mpsc, watch};
    use tokio_tungstenite::tungstenite::protocol::Message;
    use tokio_tungstenite::WebSocketStream;
    use tokio_tungstenite::{accept_async, tungstenite::Error};
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    async fn ws_sender_f(
        peer_id: usize,
        mut sink: SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>,
        mut map_receiver: watch::Receiver<State>,
    ) {
        while map_receiver.changed().await.is_ok() {
            let mut val = map_receiver.borrow_and_update().clone();
            val.0 = peer_id;
            let json = serde_json::to_string(&val).expect("json serialization");
            if (sink.send(Message::text(json)).await).is_err() {
                break;
            }
        }
    }

    async fn ws_receiver_f(
        peer_id: usize,
        mut stream: SplitStream<WebSocketStream<TcpStream>>,
        action_sender: mpsc::Sender<ShipAction>,
    ) {
        while let Some(Ok(msg)) = stream.next().await {
            if let Message::Text(text) = msg {
                if let Ok(action) = serde_json::from_str::<Action>(&text) {
                    if (action_sender.send(ShipAction::new(peer_id, action)).await).is_err() {
                        break;
                    }
                }
            }
        }
    }

    async fn accept_connection(
        stream: TcpStream,
        peer_id: usize,
        action_sender: mpsc::Sender<ShipAction>,
        map_receiver: watch::Receiver<State>,
    ) {
        match accept_async(stream).await {
            Ok(ws_stream) => {
                let (ws_sender, ws_receiver) = ws_stream.split();
                tokio::spawn(ws_receiver_f(peer_id, ws_receiver, action_sender));
                tokio::spawn(ws_sender_f(peer_id, ws_sender, map_receiver));
            }
            Err(e) => match e {
                Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
                err => eprintln!("Error processing connection: {}", err),
            },
        }
    }

    pub async fn start_server(action_sender: mpsc::Sender<ShipAction>, map_receiver: watch::Receiver<State>) {
        let ws_addr = SocketAddr::from(([0, 0, 0, 0], 48666));
        let listener = TcpListener::bind(ws_addr).await.expect("Failed to bind");
        println!("Listening on: {}", ws_addr);

        let mut id = 1usize;
        while let Ok((stream, _)) = listener.accept().await {
            if let Ok(addr) = stream.peer_addr() {
                println!("New connection: {id} - {addr}");
                tokio::spawn(accept_connection(
                    stream,
                    id,
                    action_sender.clone(),
                    map_receiver.clone(),
                ));
                id += 1;
            }
        }
    }
}

use game::{run, ShipAction};
use state::State;
use wserver::start_server;

#[tokio::main]
async fn main() {
    let (action_sender, action_receiver) = mpsc::channel::<ShipAction>(32);
    let (map_sender, map_receiver) = watch::channel::<State>(State::default());
    // let (map_sender, mut map_receiver) = watch::channel::<State>(State(vec![], vec![], [[0i8; 20]; 20]));

    tokio::spawn(async move {
        start_server(action_sender, map_receiver).await
    });
    run(action_receiver, map_sender).await;
}
