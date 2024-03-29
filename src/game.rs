use std::cmp;
use rand::Rng;
use rand::distributions::{Weighted, WeightedChoice, IndependentSample};
use tcod::colors;
use tcod::console::*;
use tcod::input::{self, Key, Event, Mouse};
use tcod::map::{FovAlgorithm, Map as FovMap};

use std::io::{Read, Write};
use std::fs::File;
use std::error::Error;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;
const PLAYER: usize = 0;

/// Mutably borrow two separate entities from the given slice
/// Panics when indexes are equal or out of bounds
fn mut_two<T>(first: usize, second: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first != second);
    let split_at_index = cmp::max(first, second);
    let (first_slice, second_slice) = items.split_at_mut(split_at_index);
    if first < second {
        (&mut first_slice[first], &mut second_slice[0])
    } else {
        (&mut second_slice[0], &mut first_slice[second])
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Tile {
    blocked: bool,
    block_sight: bool,
    explored: bool,
}

impl Tile {
    pub fn empty() -> Self {
        Tile{ blocked: false, block_sight: false, explored: false, }
    }
    pub fn wall() -> Self {
        Tile{ blocked: true, block_sight: true, explored: false, }
    }
    pub fn bushes() -> Self {
        Tile{ blocked: false, block_sight: true, explored: false, }
    }
}

type Map = Vec<Vec<Tile>>;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Fighter {
    hp: i32,
    base_max_hp: i32,
    base_defence: i32,
    base_power: i32,
    xp: i32,
    on_death: DeathCallback,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Ai {
    Basic,
    Confused {
        previous_ai: Box<Ai>,
        num_turns: i32,
    },
}


#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum DeathCallback {
    Player,
    Monster,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum Item {
    Heal,
    Lightning,
    Confuse,
    Fireball,
    Sword,
    Shield,
    Helmet,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Equipment {
    slot: Slot,
    equipped: bool,
    power_bonus: i32,
    defence_bonus: i32,
    max_hp_bonus: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum Slot {
    LeftHand,
    RightHand,
    Head,
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Slot::LeftHand => write!(f, "left hand"),
            Slot::RightHand => write!(f, "right hand"),
            Slot::Head => write!(f, "head"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Object {
    name: String,

    x: i32,
    y: i32,

    char: char,
    color: colors::Color,
    blocks: bool,
    always_visible: bool,
    alive: bool,

    fighter: Option<Fighter>,
    ai: Option<Ai>,
    item: Option<Item>,
    equipment: Option<Equipment>,

    level: i32,
}

impl Object {
    pub fn new(name: &str, x: i32, y: i32, char: char, color: colors::Color, blocks: bool) -> Self {
        Object {
            name: name.into(),
            x: x, y: y,
            char: char,
            color: color,
            blocks: blocks,
            always_visible: false,
            alive: false,
            fighter: None,
            ai: None,
            item: None,
            equipment: None,
            level: 1,
        }
    }

    pub fn draw(&self, con: &mut Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        if x < 0 || y < 0 || x > MAP_WIDTH || y > MAP_HEIGHT {
            return
        }
        self.x = x;
        self.y = y;
    }

    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    pub fn distance(&self, x: i32, y: i32) -> f32 {
        (((x - self.x).pow(2) + (y - self.y).pow(2)) as f32).sqrt()
    }

    pub fn take_damage(&mut self, damage: i32, messages: &mut Messages) -> Option<i32> {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }
        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self, messages);
                return Some(fighter.xp);
            }
        }
        None
    }

    pub fn attack(&mut self, target: &mut Object, game: &mut Game) {
        let damage = self.power(game) - target.defence(game);
        if damage > 0 {
            game.log.add(
                format!("{} swings and hits {} for {} damage!", self.name, target.name, damage),
                colors::WHITE);
            if let Some(xp) = target.take_damage(damage, &mut game.log) {
                self.fighter.as_mut().unwrap().xp += xp;
            }
        } else {
            game.log.add(
                format!("{} attacks the {} but it has no effect!", self.name, target.name),
                colors::WHITE);
        }
    }

    pub fn heal(&mut self, amount: i32, game: &Game) {
        let max_hp = self.max_hp(game);
        if let Some(ref mut fighter) = self.fighter {
            fighter.hp += amount;
            if fighter.hp > max_hp {
                fighter.hp = max_hp;
            }
        }
    }

    pub fn equip(&mut self, log: &mut Vec<(String, colors::Color)>) {
        if self.item.is_none() {
            log.add(format!("Can't equip {:?} as it is not an item.", self),
                colors::RED,
            );
            return;
        }
        if let Some(ref mut equipment) = self.equipment {
            if !equipment.equipped {
                equipment.equipped = true;
                log.add(format!("Equipped {} on {}.", self.name, equipment.slot),
                    colors::LIGHT_GREEN,
                );
            }
        } else {
            log.add(format!("Can't equip {:?} as it is not an equipment.", self),
                colors::RED,
            );
        }
    }

    pub fn unequip(&mut self, log: &mut Vec<(String, colors::Color)>) {
        if self.item.is_none() {
            log.add(format!("Can't unequip {:?} as it's not an item", self),
                colors::RED,
            );
            return;
        }
        if let Some(ref mut equipment) = self.equipment {
            if equipment.equipped {
                equipment.equipped = false;
                log.add(format!("Unequipped {} on {}.", self.name, equipment.slot),
                    colors::LIGHT_YELLOW,
                );
            }
        } else {
            log.add(format!("Can't unequip {:?} as it is not an equipment.", self),
                colors::RED,
            );
        }
    }

    pub fn power(&self, game: &Game) -> i32 {
        let base_power = self.fighter.map_or(0, |f| f.base_power);
        let bonus: i32 = self
            .get_all_equipped(game)
            .iter()
            .map(|e| e.power_bonus)
            .sum();
        base_power + bonus
    }

    pub fn defence(&self, game: &Game) -> i32 {
        let base_defence = self.fighter.map_or(0, |f| f.base_defence);
        let bonus: i32 = self
            .get_all_equipped(game)
            .iter()
            .map(|e| e.defence_bonus)
            .sum();
        base_defence + bonus
    }

    pub fn max_hp(&self, game: &Game) -> i32 {
        let base_max_hp = self.fighter.map_or(0, |f| f.base_max_hp);
        let bonus: i32 = self
            .get_all_equipped(game)
            .iter()
            .map(|e| e.max_hp_bonus)
            .sum();
        base_max_hp + bonus
    }

    pub fn get_all_equipped(&self, game: &Game) -> Vec<Equipment> {
        if self.name == "Player" {
            game.inventory
                .iter()
                .filter(|item| item.equipment.map_or(false, |e| e.equipped))
                .map(|item| item.equipment.unwrap())
                .collect()
        } else {
            vec![]
        }
    }
}

type Messages = Vec<(String, colors::Color)>;

#[derive(Serialize, Deserialize)]
struct Game {
    map: Map,
    log: Messages,
    inventory: Vec<Object>,
    dungeon_level: u32,
}

trait MessageLog {
    fn add<T: Into<String>>(&mut self, message: T, color: colors::Color);
}

impl MessageLog for Messages {
    fn add<T: Into<String>>(&mut self, message: T, color: colors::Color) {
        self.push((message.into(), color));
    }
}


impl DeathCallback {
    fn callback(self, object: &mut Object, messages: &mut Messages) {
        use DeathCallback::*;
        let callback: fn(&mut Object, &mut Messages) = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(object, messages);
    }
}

fn player_death(player: &mut Object, messages: &mut Messages) {
    messages.add("You die!", colors::RED);

    player.char = '%';
    player.color = colors::DARK_RED;
}

fn monster_death(monster: &mut Object, messages: &mut Messages) {
    messages.add(
        format!("{} dies! You gain {} XP.",
                monster.name, monster.fighter.as_mut().unwrap().xp),
        colors::ORANGE);
    monster.char = '%';
    monster.color = colors::DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("Remains of {}", monster.name);
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    if x < 0 || y < 0 || x >= MAP_WIDTH || y >= MAP_HEIGHT {
        return true;
    }
    if map[x as usize][y as usize].blocked {
        return true;
    }
    objects.iter().any(|object| {
        object.blocks && object.pos() == (x, y)
    })
}

fn move_by(id: usize, dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let (x, y) = objects[id].pos();
    if !is_blocked(x+dx, y+dy, map, objects) {
        objects[id].set_pos(x+dx, y+dy);
    }
}

fn ai_take_turn(monster_id: usize, game: &mut Game, objects: &mut [Object], fov_map: &FovMap) {
    use Ai::*;

    if let Some(ai) = objects[monster_id].ai.take() {
        let new_ai = match ai {
            Basic => ai_basic(monster_id, game, objects, fov_map),
            Confused {
                previous_ai,
                num_turns,
            } => ai_confused(monster_id, game, objects, previous_ai, num_turns),
        };
        objects[monster_id].ai = Some(new_ai);
    }
}

fn ai_basic(
    monster_id: usize,
    game: &mut Game,
    objects: &mut [Object],
    fov_map: &FovMap,
) -> Ai {
    let (monster_x, monster_y) = objects[monster_id].pos();
    if fov_map.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, &game.map, objects);
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.attack(player, game);
        }
    } else {
        let choices = [-1, 0, 1];
        let tx = match rand::thread_rng().choose(&choices) {
            Some(dx) => monster_x + dx,
            _ => monster_x,
        };
        let ty = match rand::thread_rng().choose(&choices) {
            Some(dy) => monster_y + dy,
            _ => monster_y,
        };
        move_towards(monster_id, tx, ty, &game.map, objects);
    }
    Ai::Basic
}

fn ai_confused(
    monster_id: usize,
    game: &mut Game,
    objects: &mut [Object],
    previous_ai: Box<Ai>,
    num_turns: i32,
) -> Ai {
    if num_turns < 0 {
        game.log.add(format!("{} is no longer confused!", objects[monster_id].name),
            colors::RED);
        *previous_ai
    } else {
        move_by(monster_id,
                rand::thread_rng().gen_range(-1, 2),
                rand::thread_rng().gen_range(-1, 2),
                &mut game.map,
                objects,
        );
        Ai::Confused {
            previous_ai: previous_ai,
            num_turns: num_turns - 1,
        }
    }
}

fn get_equipped_in_slot(slot: Slot, inventory: &[Object]) -> Option<usize> {
    for (inventory_id, item) in inventory.iter().enumerate() {
        if item.equipment.as_ref().map_or(false, |e| e.equipped && e.slot == slot) {
            return Some(inventory_id);
        }
    }
    None
}

//////////////////////// MAPGEN
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;

#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32, y1: i32,
    x2: i32, y2: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect{x1: x, y1: y, x2: x + w, y2: y + h}
    }
    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x2 + self.x1) / 2;
        let center_y = (self.y2 + self.y1) / 2;
        (center_x, center_y)
    }
    pub fn intersects_with(&self, other: &Rect) -> bool {
        (self.x1 <= other.x2) && (self.x2 >= other.x1) &&
            (self.y1 <= other.y2) && (self.y2 >= other.y1)
    }
}

fn create_room(room: Rect, map: &mut Map, objects: &mut Vec<Object>, first_room: bool, level: u32) {
    // Just a rectangle
    let tiles = &mut [
        Weighted { item: false, weight: 10 },
        Weighted { item: true, weight: 90 },
    ];
    let tiles_choices = WeightedChoice::new(tiles);
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            let tile = match tiles_choices.ind_sample(&mut rand::thread_rng()) {
                false => Tile::bushes(),
                true => Tile::empty(),
            };
            map[x as usize][y as usize] = tile;
        }
    }
    // Let's add some chaos to the boring rectangular room
    let wall_burrow = &mut [
        Weighted { item: true, weight: 60 },
        Weighted { item: false, weight: 40 },
    ];
    let wall_burrow_choice = WeightedChoice::new(wall_burrow);
    let mut walls = vec![];
    for x in (room.x1 + 1)..(room.x2 - 1) {
        if x <= 0 || x >= MAP_WIDTH {
            continue;
        }
        walls.push((x, room.y1));
        walls.push((x, room.y2));
    }
    for y in (room.y1 + 1)..(room.y2 - 1) {
        if y <= 0 || y >= MAP_HEIGHT {
            continue;
        }
        walls.push((room.x1, y));
        walls.push((room.x2, y));
    }
    for (wall_x, wall_y) in walls {
        match wall_burrow_choice.ind_sample(&mut rand::thread_rng()) {
            false => map[wall_x as usize][wall_y as usize] = Tile::empty(),
            _ => {}
        };
    }

    // Place all the player, monsters and items
    place_objects(room, objects, map, first_room, level)
}

fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn create_v_tunnel(x: i32, y1: i32, y2: i32, map: &mut Map) {
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn make_map(objects: &mut Vec<Object>, level: u32) -> Map {
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    assert_eq!(&objects[PLAYER] as *const _, &objects[0] as *const _);
    objects.truncate(1);

    // Generate the map as a series of rooms connected with tunnels
    let mut rooms = vec![];
    for _ in 0..MAX_ROOMS {
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);
        let new_room = Rect::new(x, y, w, h);

        let failed = rooms
            .iter()
            .any(|other_room| new_room.intersects_with(other_room));

        if !failed {
            create_room(new_room, &mut map, objects, rooms.is_empty(), level);
            let (new_x, new_y) = new_room.center();
            if rooms.is_empty() {
                objects[PLAYER].set_pos(new_x, new_y);
            } else {
                let (prev_x, prev_y) = rooms[rooms.len() - 1].center();

                if rand::random() {
                    // Horizontal then vertical
                    create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                    create_v_tunnel(new_x, prev_y, new_y, &mut map);
                } else {
                    // Vertical then horizontal
                    create_v_tunnel(prev_x, prev_y, new_y, &mut map);
                    create_h_tunnel(prev_x, new_x, new_y, &mut map);
                }
            }

            rooms.push(new_room);
        }
    }
    // Relax some of the rough edges
    for x in 2..MAP_WIDTH-2 {
        for y in 2..MAP_HEIGHT-2 {
            let mut walls = 0;
            for dx in (x - 1) .. (x + 2) {
                for dy in (y - 1) .. (y + 2) {
                    let tile = map[dx as usize][dy as usize];
                    if tile.blocked && tile.block_sight {
                        walls += 1;
                    }
                }
            }
            let tile = map[x as usize][y as usize];
            if walls < 5 && tile.blocked {
                map[x as usize][y as usize] = Tile::empty();
            }
        }
    }

    let (last_room_x, last_room_y) = rooms[rooms.len() - 1].center();
    let mut stairs = Object::new(
        "stairs down",
        last_room_x, last_room_y,
        '<',
        colors::WHITE,
        false,
    );
    stairs.always_visible = true;
    objects.push(stairs);
    map
}

struct Transition {
    level: u32,
    value: u32,
}

/// Returns a value that depends on level
fn from_dungeon_level(table: &[Transition], level: u32) -> u32 {
    table
        .iter()
        .rev()
        .find(|transition| level >= transition.level)
        .map_or(0, |transition| transition.value)
}

/////////////////////// Logic
const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 10;

const HEAL_AMOUNT: i32 = 40;
const LIGHTNING_RANGE: i32 = 5;
const LIGHTNING_DAMAGE: i32 = 40;
const CONFUSE_RANGE: i32 = 10;
const CONFUSE_NUM_TURNS: i32 = 8;
const FIREBALL_RADIUS: i32 = 3;
const FIREBALL_DAMAGE: i32 = 25;
const LEVEL_UP_BASE: i32 = 200;
const LEVEL_UP_FACTOR: i32 = 150;

fn place_objects(room: Rect, objects: &mut Vec<Object>, map: &Map, first_room: bool, level: u32) {
    let item_chances = &mut [
        Weighted { item: Item::Heal, weight: 35 },
        Weighted { item: Item::Lightning,
                    weight: from_dungeon_level(
                        &[Transition { level: 4, value: 25 }, ],
                        level
                    )},
        Weighted { item: Item::Fireball,
                    weight: from_dungeon_level(
                        &[Transition { level: 6, value: 25 }, ],
                        level
                    )},
        Weighted { item: Item::Confuse,
                    weight: from_dungeon_level(
                        &[Transition { level: 2, value: 10 }, ],
                        level
                    )},
        Weighted { item: Item::Sword,
                    weight: from_dungeon_level(
                        &[Transition { level: 4, value: 5 }, ],
                        level
                    )},
        Weighted { item: Item::Shield,
                    weight: from_dungeon_level(
                        &[Transition { level: 8, value: 15 }, ],
                        level
                    )},
        Weighted { item: Item::Helmet,
                    weight: from_dungeon_level(
                        &[Transition { level: 5, value: 20 }, ],
                        level
                    )},
    ];
    let item_choice = WeightedChoice::new(item_chances);

    let max_items = from_dungeon_level(
        &[Transition { level: 1, value: 1 },
          Transition { level: 4, value: 2 }, ],
        level,
    );
    let num_items = rand::thread_rng().gen_range(0, max_items + 1);
    for _ in 0..num_items {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);
        if !is_blocked(x, y, map, objects) {
            let item = match item_choice.ind_sample(&mut rand::thread_rng()) {
                Item::Heal => {
                    let mut object = Object::new("healing potion", x, y,
                                                 '!', colors::VIOLET, false);
                    object.item = Some(Item::Heal);
                    object
                }
                Item::Lightning => {
                    let mut object = Object::new("scroll of lightning", x, y,
                                                 '#', colors::LIGHT_YELLOW, false);
                    object.item = Some(Item::Lightning);
                    object
                }
                Item::Fireball => {
                    let mut object = Object::new("scroll of fireball", x, y,
                                                 '#', colors::LIGHT_YELLOW, false);
                    object.item = Some(Item::Fireball);
                    object
                }
                Item::Confuse => {
                    let mut object = Object::new("scroll of confusion", x, y,
                                                 '&', colors::LIGHT_YELLOW, false);
                    object.item = Some(Item::Confuse);
                    object
                }
                Item::Sword => {
                    let mut object = Object::new("sword", x, y, '/', colors::SKY, false);
                    object.item = Some(Item::Sword);
                    object.equipment = Some(Equipment{
                        equipped: false, power_bonus: 3, defence_bonus: 0, max_hp_bonus: 1,
                        slot: Slot::RightHand});
                    object
                }
                Item::Shield => {
                    let mut object = Object::new("shield", x, y, '*', colors::DARKER_ORANGE, false);
                    object.item = Some(Item::Shield);
                    object.equipment = Some(Equipment{
                        equipped: false, power_bonus: 0, defence_bonus: 1, max_hp_bonus: 1,
                        slot: Slot::LeftHand});
                    object
                }
                Item::Helmet => {
                    let mut object = Object::new("helmet", x, y, '^', colors::DARKER_ORANGE, false);
                    object.item = Some(Item::Helmet);
                    object.equipment = Some(Equipment{
                        equipped: false, power_bonus: 0, defence_bonus: 0,
                        max_hp_bonus: 100,
                        slot: Slot::Head});
                    object
                }
            };
            objects.push(item);
        }
    }

    if first_room {
        return
    }

    let troll_chance = from_dungeon_level(
        &[Transition { level: 3, value: 15, },
          Transition { level: 5, value: 30, },
          Transition { level: 7, value: 60, }, ],
        level,
    );
    let monster_chances = &mut [
        Weighted { weight: 80, item: "orc", },
        Weighted { weight: troll_chance, item: "troll", },
    ];
    let monster_choice = WeightedChoice::new(monster_chances);

    let max_monsters = from_dungeon_level(
        &[Transition { level: 1, value: 2},
          Transition { level: 4, value: 3},
          Transition { level: 6, value: 5}, ],
        level,
    );
    let num_monsters = rand::thread_rng().gen_range(0, max_monsters + 1);
    for _ in 0..num_monsters {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);
        if is_blocked(x, y, map, objects) {
            continue;
        }
        let mut monster = match monster_choice.ind_sample(&mut rand::thread_rng()) {
            "orc" => {
                let mut orc = Object::new("Orc", x, y, '0', colors::LIGHT_GREEN, true);
                orc.fighter = Some(Fighter {
                    hp: 20,
                    base_max_hp: 20,
                    base_defence: 0,
                    base_power: 4,
                    xp: 35,
                    on_death: DeathCallback::Monster,
                });
                orc.ai = Some(Ai::Basic);
                orc
            }
            "troll" => {
                let mut troll = Object::new("Troll", x, y, 'T', colors::RED, true);
                troll.fighter = Some(Fighter {
                    hp: 30,
                    base_max_hp: 30,
                    base_defence: 2,
                    base_power: 8,
                    xp: 100,
                    on_death: DeathCallback::Monster,
                });
                troll.ai = Some(Ai::Basic);
                troll
            }
            _ => unreachable!(),
        };
        monster.alive = true;
        objects.push(monster);
    }
}

fn next_level(tcod: &mut Tcod, objects: &mut Vec<Object>, game: &mut Game) {
    game.log.add("You take a moment to rest and recover your strength.",
                 colors::VIOLET,
                 );
    let heal_hp = objects[PLAYER].max_hp(game) / 2;
    objects[PLAYER].heal(heal_hp, &game);

    game.log.add("After a rare moment of peace you descend deeper into the sewers...",
                 colors::RED,
                 );
    game.dungeon_level += 1;
    game.map = make_map(objects, game.dungeon_level);
    initialise_fov(tcod, &game.map);
}

fn pick_item_up(
    object_id: usize,
    objects: &mut Vec<Object>,
    game: &mut Game,
) {
    if game.inventory.len() >= 26 {
        game.log.add(format!("Your inventory is full, cannot pick up {}.", objects[object_id].name),
                     colors::RED);
    } else {
        let item = objects.swap_remove(object_id);
        game.log.add(format!("You picked up a {}!", item.name), colors::GREEN);
        let index = game.inventory.len();
        let slot = item.equipment.map(|e| e.slot);
        game.inventory.push(item);

        if let Some(slot) = slot {
            if get_equipped_in_slot(slot, &game.inventory).is_none() {
                game.inventory[index].equip(&mut game.log);
            }
        }
    }
}

fn level_up(objects: &mut [Object], game: &mut Game, tcod: &mut Tcod) {
    let player = &mut objects[PLAYER];
    let level_up_xp = LEVEL_UP_BASE + player.level * LEVEL_UP_FACTOR;
    if player.fighter.as_ref().map_or(0, |f| f.xp) >= level_up_xp {
        player.level += 1;
        game.log.add(
            format!("Your battle skills grow stronger! You've reached level {}!",
                    player.level),
            colors::YELLOW,
        );

        let fighter = player.fighter.as_mut().unwrap();
        let mut choice = None;
        while choice.is_none() {
            choice = menu(
                "Level up! Choose a stat to raise!\n",
                &[format!("Constitution (+20 HP from {})", fighter.base_max_hp),
                  format!("Strength (+1 attack from {})", fighter.base_power),
                  format!("Agility (+1 defence from {})", fighter.base_defence),
                ],
                LEVEL_SCREEN_WIDTH,
                &mut tcod.root,
                );
        }
        fighter.xp -= level_up_xp;
        match choice.unwrap() {
            0 => {
                fighter.base_max_hp += 20;
                fighter.hp += 20;
            }
            1 => {
                fighter.base_power += 1;
            }
            2 => {
                fighter.base_defence += 1;
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

fn player_move_or_attack(dx: i32, dy: i32, game: &mut Game, objects: &mut [Object]) {
    let x = objects[PLAYER].x + dx;
    let y = objects[PLAYER].y + dy;

    let target_id = objects.iter().position(|object| object.fighter.is_some() && object.pos() == (x, y));
    match target_id {
        Some(target_id) => {
            let (player, monster) = mut_two(PLAYER, target_id, objects);
            player.attack(monster, game);
        }
        None => {
            move_by(PLAYER, dx, dy, &mut game.map, objects)
        }
    }
}

fn move_towards(id: usize, target_x: i32, target_y: i32, map: &Map, objects: &mut [Object]) {
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    // Add some drift to help monsters move around the corners
    if rand::random() {
        move_by(id, dx, dy, map, objects);
    } else {
        move_by(id, dy, dx, map, objects);
    }
}

fn closest_monster(max_range: i32, objects: &mut [Object], tcod: &Tcod) -> Option<usize> {
    let mut closest_enemy = None;
    let mut closest_dist = (max_range + 1) as f32;

    for (id, object) in objects.iter().enumerate() {
        if id != PLAYER && object.fighter.is_some() && object.ai.is_some() &&
            tcod.fov.is_in_fov(object.x, object.y) {
            let dist = objects[PLAYER].distance_to(object);
            if dist < closest_dist {
                closest_enemy = Some(id);
                closest_dist = dist;
            }
        }
    }
    closest_enemy
}

enum UseResult {
    UsedUp,
    Cancelled,
    UsedAndKept,
}

fn cast_heal(
    _inventory_id: usize,
    game: &mut Game,
    objects: &mut [Object],
    _tcod: &mut Tcod
) -> UseResult {
    let player = &mut objects[PLAYER];
    if let Some(fighter) = player.fighter {
        if fighter.hp == objects[PLAYER].max_hp(game) {
            game.log.add("You are already at full health.", colors::RED);
            return UseResult::Cancelled;
        }
        game.log.add("Your wounds are healing!", colors::LIGHT_VIOLET);
        objects[PLAYER].heal(HEAL_AMOUNT, &game);
        return UseResult::UsedUp;
    }
    UseResult::Cancelled
}

fn cast_lightning(
    _inventory_id: usize,
    game: &mut Game,
    objects: &mut [Object],
    tcod: &mut Tcod,
) -> UseResult {
    let monster_id = closest_monster(LIGHTNING_RANGE, objects, tcod);
    if let Some(monster_id) = monster_id {
        game.log.add(format!("A lightning strikes {} with a loud thunder for {} damage!",
                             objects[monster_id].name, LIGHTNING_DAMAGE),
                colors::LIGHT_BLUE,
        );
        if let Some(xp) = objects[monster_id].take_damage(LIGHTNING_DAMAGE, &mut game.log) {
            objects[PLAYER].fighter.as_mut().unwrap().xp += xp;
        }
        UseResult::UsedUp
    } else {
        game.log.add("No enemy is close enough to strike.", colors::RED);
        UseResult::Cancelled
    }
}

fn cast_confuse(
    _inventory_id: usize,
    game: &mut Game,
    objects: &mut [Object],
    tcod: &mut Tcod,
) -> UseResult {
    game.log.add(
        "Left click an enemy to confuse it, or right click to cancel.",
        colors::LIGHT_CYAN);
    let monster_id = target_monster(tcod, game, objects, Some(CONFUSE_RANGE as f32));
    if let Some(monster_id) = monster_id {
        let old_ai = objects[monster_id].ai.take().unwrap_or(Ai::Basic);
        objects[monster_id].ai = Some(Ai::Confused {
            previous_ai: Box::new(old_ai),
            num_turns: CONFUSE_NUM_TURNS,
        });
        game.log.add(
            format!("{} starts stumbling around!", objects[monster_id].name),
            colors::LIGHT_GREEN,
        );
        UseResult::UsedUp
    } else {
        game.log.add("No enemy is close enought to strike.", colors::RED);
        UseResult::Cancelled
    }
}

fn cast_fireball(
    _inventory_id: usize,
    game: &mut Game,
    objects: &mut [Object],
    tcod: &mut Tcod,
) -> UseResult {
    game.log.add(
        "Left click to target tile for the fireball, right click to cancel",
        colors::LIGHT_CYAN);
    let (x, y) = match target_tile(tcod, game, objects, None) {
        Some(tile_pos) => tile_pos,
        None => return UseResult::Cancelled,
    };

    game.log.add(
        format!("The fireball explodes, burning everything within {} tiles!", FIREBALL_RADIUS),
        colors::ORANGE);

    let mut xp_to_gain = 0;
    for (id, obj) in objects.iter_mut().enumerate() {
        if obj.distance(x, y) <= FIREBALL_RADIUS as f32 && obj.fighter.is_some() {
            game.log.add(
                format!("The {} gets burned for {} hit points.", obj.name, FIREBALL_DAMAGE),
                colors::ORANGE);
            if let Some(xp) = obj.take_damage(FIREBALL_DAMAGE, &mut game.log) {
                if id != PLAYER {
                    xp_to_gain += xp;
                }
            }
        }
    }
    objects[PLAYER].fighter.as_mut().unwrap().xp += xp_to_gain;
    UseResult::UsedUp
}

fn toggle_equipment(inventory_id: usize, game: &mut Game, _objects: &mut [Object], _tcod: &mut Tcod) -> UseResult {
    let equipment = match game.inventory[inventory_id].equipment {
        Some(equipment) => equipment,
        None => return UseResult::Cancelled,
    };
    if let Some(old_equipment) = get_equipped_in_slot(equipment.slot, &game.inventory) {
        game.inventory[old_equipment].unequip(&mut game.log);
    }

    if equipment.equipped {
        game.inventory[inventory_id].unequip(&mut game.log);
    } else {
        game.inventory[inventory_id].equip(& mut game.log);
    }
    UseResult::UsedAndKept
}

fn use_item(
    inventory_id: usize,
    game: &mut Game,
    objects: &mut [Object],
    tcod: &mut Tcod,
) {
    use Item::*;
    if let Some(item) = game.inventory[inventory_id].item {
        let on_use = match item {
            Heal => cast_heal,
            Lightning => cast_lightning,
            Confuse => cast_confuse,
            Fireball => cast_fireball,
            Sword => toggle_equipment,
            Shield => toggle_equipment,
            Helmet => toggle_equipment,
        };
        match on_use(inventory_id, game, objects, tcod) {
            UseResult::UsedUp => {
                // destroy after use
                game.inventory.remove(inventory_id);
            }
            UseResult::Cancelled => {
                game.log.add("Cancelled", colors::WHITE);
            }
            UseResult::UsedAndKept => {}
        }
    } else {
        game.log.add(
            format!("The {} cannot be used.", game.inventory[inventory_id].name),
            colors::WHITE);
    }
}

fn drop_item(
    inventory_id: usize,
    game: &mut Game,
    objects: &mut Vec<Object>,
) {
    let mut item = game.inventory.remove(inventory_id);
    if item.equipment.is_some() {
        item.unequip(&mut game.log);
    }
    item.set_pos(objects[PLAYER].x, objects[PLAYER].y);
    game.log.add(format!("You dropped a {}.", item.name), colors::YELLOW);
    objects.push(item);
}

///////////////////////////////// UI Work
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;
const LEVEL_SCREEN_WIDTH: i32 = 40;
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;
const COLOR_DARK_WALL: colors::Color = colors::Color    { r: 20,     g: 20,   b: 20 };
const COLOR_LIGHT_WALL: colors::Color = colors::Color   { r: 80,   g: 80, b: 50 };
const COLOR_DARK_GROUND: colors::Color = colors::Color  { r: 80,    g: 50,  b: 50 };
const COLOR_LIGHT_GROUND: colors::Color = colors::Color { r: 130,   g: 130, b: 80 };
const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;
const INVENTORY_WIDTH: i32 = 50;
const CHARACTER_SCREEN_WIDTH: i32 = 30;

const LIMIT_FPS: i32 = 60;

struct Tcod {
    root: Root,
    con: Offscreen,
    panel: Offscreen,
    fov: FovMap,
    mouse: Mouse,
}

fn handle_keys(key: Key,
               tcod: &mut Tcod,
               objects: &mut Vec<Object>,
               game: &mut Game,
) -> PlayerAction {
    use PlayerAction::*;
    use tcod::input::Key;
    use tcod::input::KeyCode::*;

    let alive = objects[0].alive;
    let action = match (key, alive) {
        (Key { code: Enter, alt: true, .. }, _) => {
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
            DidntTakeTurn
        }
        (Key { code: Escape, .. }, _) => return Exit,
        (Key { code: Up, .. }, true) | (Key { code: NumPad8, .. }, true) => {
            player_move_or_attack(0, -1, game, objects);
            TookTurn
        }
        (Key { code: Down, .. }, true) | (Key { code: NumPad2, .. }, true) => {
            player_move_or_attack(0, 1, game, objects);
            TookTurn
        }
        (Key { code: Left, .. }, true) | (Key { code: NumPad4, .. }, true) => {
            player_move_or_attack(-1, 0, game, objects);
            TookTurn
        }
        (Key { code: Right, .. }, true) | (Key { code: NumPad6, .. }, true) => {
            player_move_or_attack(1, 0, game, objects);
            TookTurn
        }
        (Key { code: Home, .. }, true) | (Key { code: NumPad7, .. }, true) => {
            player_move_or_attack(-1, -1, game, objects);
            TookTurn
        }
        (Key { code: PageUp, .. }, true) | (Key { code: NumPad9, .. }, true) => {
            player_move_or_attack(1, -1, game, objects);
            TookTurn
        }
        (Key { code: End, .. }, true) | (Key { code: NumPad1, .. }, true) => {
            player_move_or_attack(-1, 1, game, objects);
            TookTurn
        }
        (Key { code: PageDown, .. }, true) | (Key { code: NumPad3, .. }, true) => {
            player_move_or_attack(1, 1, game, objects);
            TookTurn
        }
        (Key { code: NumPad5, .. }, true) | (Key { code: Spacebar, .. }, true) => {
            // Wait for a turn
            TookTurn
        }
        (Key {printable: 'g', .. }, true) => {
            let item_id = objects
                .iter()
                .position(|object| object.pos() == objects[PLAYER].pos() &&
                          object.item.is_some());
            if let Some(item_id) = item_id {
                pick_item_up(item_id, objects, game);
            }
            DidntTakeTurn
        }
        (Key {printable: 'i', .. }, true) => {
            let inventory_index = inventory_menu(
                &game.inventory,
                "Press the key next to an item to use it, any other to cancel.\n",
                &mut tcod.root);
            if let Some(inventory_index) = inventory_index {
                use_item(inventory_index, game, objects, tcod);
            }
            DidntTakeTurn
        }
        (Key { printable: 'd', .. }, true) => {
            let inventory_index = inventory_menu(
                &game.inventory,
                "Select and item to drop\n",
                &mut tcod.root);
            if let Some(inventory_index) = inventory_index {
                drop_item(inventory_index, game, objects);
            }
            DidntTakeTurn
        }
        (Key { printable: 'c', .. }, true) => {
            let player = &objects[PLAYER];
            let level = player.level;
            let level_up_xp = LEVEL_UP_BASE + player.level * LEVEL_UP_FACTOR;
            if let Some(fighter) = player.fighter.as_ref() {
                let msg = format!(
                    "Character information

Level: {}
Experience: {}
Next level up: {}

Max HP: {}
Attack: {}
Defence: {}",
                    level, fighter.xp, level_up_xp, player.max_hp(game),
                    player.power(game), player.defence(game));
                msgbox(&msg, CHARACTER_SCREEN_WIDTH, &mut tcod.root);
            }
            DidntTakeTurn
        }
        (Key { printable: ',', shift: true, .. }, true) => {
            let player_on_stairs = objects
                .iter()
                .any(|object| object.pos() == objects[PLAYER].pos() && object.name == "stairs down");
            if player_on_stairs {
                next_level(tcod, objects, game);
            }
            DidntTakeTurn
        }
        (Key {printable: '=', .. }, true) => {
            uncover_map(game);
            DidntTakeTurn
        }
        _ => DidntTakeTurn
    };
    action
}

/// return the position of a tile left-clicked in the player's FOV
/// or (None, None) if right-clicked
fn target_tile(
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &[Object],
    max_range: Option<f32>,
) -> Option<(i32, i32)> {
    use tcod::input::KeyCode::Escape;
    loop {
        // render the screen (to erase inventory) and show objects under cursor
        tcod.root.flush();
        let event = input::check_for_event(input::KEY_PRESS | input::MOUSE).map(
            |e| e.1);
        let mut key = None;
        match event {
            Some(Event::Mouse(m)) => tcod.mouse = m,
            Some(Event::Key(k)) => key = Some(k),
            None => {},
        }
        render_all(tcod, objects, game, false);
        let (x, y) = (tcod.mouse.cx as i32, tcod.mouse.cy as i32);

        // accept the target if the player clicked in fov, filter by range is specified
        let in_fov = x >= 0 && y >= 0 && x < MAP_WIDTH && y < MAP_HEIGHT && tcod.fov.is_in_fov(x, y);
        let in_range = max_range.map_or(true, |range| objects[PLAYER].distance(x, y) <= range);
        if tcod.mouse.lbutton_pressed && in_fov && in_range {
            return Some((x, y))
        }

        let escape = key.map_or(false, |k| k.code == Escape);
        if tcod.mouse.rbutton_pressed || escape {
            return None
        }
    }
}

fn target_monster(
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &[Object],
    max_range: Option<f32>,
) -> Option<usize> {
    loop {
        match target_tile(tcod, game, objects, max_range) {
            Some((x, y)) => {
                for (id, obj) in objects.iter().enumerate() {
                    if obj.pos() == (x, y) && obj.fighter.is_some() && id != PLAYER {
                        return Some(id);
                    }
                }
            },
            None => return None,
        }
    }
}

fn save_game(objects: &[Object], game: &Game) -> Result<(), Box<Error>> {
    let save_data = serde_json::to_string(&(objects, game))?;
    let mut file = File::create("savegame")?;
    file.write_all(save_data.as_bytes())?;
    Ok(())
}

fn load_game() -> Result<(Vec<Object>, Game), Box<Error>> {
    let mut json_save_state = String::new();
    let mut file = File::open("savegame")?;
    file.read_to_string(&mut json_save_state)?;
    let result = serde_json::from_str::<(Vec<Object>, Game)>(&json_save_state)?;
    Ok(result)
}

fn menu<T: AsRef<str>>(header: &str, options: &[T], width: i32, root: &mut Root) -> Option<usize> {
    assert!(options.len() <= 26, "Menu can only fit 26 options");

    let header_height = if header.is_empty() {
        0
    } else {
        root.get_height_rect(0, 0, width, SCREEN_HEIGHT, header)
    };
    let height = options.len() as i32 + header_height;

    // off-screen console representing the window
    let mut window = Offscreen::new(width, height);

    window.set_default_foreground(colors::WHITE);
    window.print_rect_ex(
        0, 0,
        width, height,
        BackgroundFlag::None,
        TextAlignment::Left,
        header,
    );

    for (index, option_text) in options.iter().enumerate() {
        let menu_letter = (b'a' + index as u8) as char;
        let text = format!("({}) {}", menu_letter, option_text.as_ref());
        window.print_ex(
            0,
            header_height + index as i32,
            BackgroundFlag::None,
            TextAlignment::Left,
            text,
        );
    }

    // blit the window to the root console
    let x = SCREEN_WIDTH / 2 - width / 2;
    let y = SCREEN_HEIGHT / 2 - height / 2;
    tcod::console::blit(&mut window, (0, 0), (width, height), root, (x, y), 1.0, 0.7);

    root.flush();
    let key = root.wait_for_keypress(true);

    // ASCII code to index
    if key.printable.is_alphabetic() {
        let index = key.printable.to_ascii_lowercase() as usize - 'a' as usize;
        if index < options.len() {
            Some(index)
        } else {
            None
        }
    } else {
        None
    }
}

fn inventory_menu(inventory: &[Object], header: &str, root: &mut Root) -> Option<usize> {
    let options = if inventory.len() == 0 {
        vec!["Inventory is empty.".into()]
    } else {
        inventory.iter().map(|item| {
            match item.equipment {
                Some(equipment) if equipment.equipped => {
                    format!("{} (on {})", item.name, equipment.slot)
                }
                _ => item.name.clone()
            }
        }).collect()
    };

    let inventory_index = menu(header, &options, INVENTORY_WIDTH, root);

    if inventory.len() > 0 {
        inventory_index
    } else {
        None
    }
}

fn get_names_under_mouse(mouse: Mouse, objects: &[Object], fov_map: &FovMap) -> String {
    let (x, y) = (mouse.cx as i32, mouse.cy as i32);

    let names = objects
        .iter()
        .filter(|obj| {obj.pos() == (x, y) && fov_map.is_in_fov(obj.x, obj.y)})
        .map(|obj| obj.name.clone())
        .collect::<Vec<_>>();

    if names.len() > 0 {
        let names_str = names.join(", ").to_string();
        let result = "You see ".to_owned() + &names_str;
        result
    } else {
        "".to_string()
    }
}

fn render_bar(
    panel: &mut Offscreen,
    x: i32,
    y: i32,
    total_width: i32,
    name: &str,
    value: i32,
    maximum: i32,
    bar_color: colors::Color,
    back_color: colors::Color
) {
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);
    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    panel.set_default_foreground(colors::WHITE);
    panel.print_ex(
        x + total_width / 2,
        y,
        BackgroundFlag::None,
        TextAlignment::Center,
        &format!("{}: {}/{}", name, value, maximum)
    );
}

fn uncover_map(game: &mut Game) {
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            game.map[x as usize][y as usize].explored = true;
        }
    }
}

fn render_all(tcod: &mut Tcod,
              objects: &[Object],
              game: &mut Game,
              fov_recompute: bool,
) {
    tcod.con.set_default_background(colors::BLACK);
    if fov_recompute {
        let player = &objects[PLAYER];
        tcod.fov.compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
    }
    // Draw map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let visible = tcod.fov.is_in_fov(x, y);
            let tile = game.map[x as usize][y as usize];
            let color = match (visible, tile.block_sight) {
                // Outside of FOV
                (false, true) => COLOR_DARK_WALL,
                (false, false) => COLOR_DARK_GROUND,
                // Inside the FOV
                (true, false) => COLOR_LIGHT_GROUND,
                (true, true) => COLOR_LIGHT_WALL,
            };
            let explored = &mut game.map[x as usize][y as usize].explored;
            if visible {
                *explored = true;
            }
            if *explored {
                if !tile.blocked && tile.block_sight {
                    // Draw a bush
                    tcod.con.set_default_foreground(colors::DARK_GREEN);
                    tcod.con.set_default_background(COLOR_DARK_GROUND);
                    tcod.con.put_char(x, y, '*', BackgroundFlag::Set);
                } else {
                    tcod.con.set_default_background(colors::BLACK);
                    tcod.con.set_char_background(x, y, color, BackgroundFlag::Set);
                }
            } else {
                tcod.con.set_default_foreground(colors::BLACK);
                tcod.con.set_default_background(colors::BLACK);
            }
        }
    }
    // Draw objects
    let mut to_draw: Vec<_> = objects.
        iter().
        filter(|obj| obj.always_visible || tcod.fov.is_in_fov(obj.x, obj.y)).
        collect();
    to_draw.sort_by(|o1, o2| { o1.blocks.cmp(&o2.blocks) });
    for object in to_draw {
        if object.always_visible || tcod.fov.is_in_fov(object.x, object.y) {
            object.draw(&mut tcod.con);
        }
    }
    blit(&mut tcod.con, (0, 0), (MAP_WIDTH, MAP_HEIGHT),
         &mut tcod.root, (0, 0),
         1.0,
         1.0);

    // Draw player stats
    tcod.panel.set_default_background(colors::BLACK);
    tcod.panel.clear();

    let mut y = MSG_HEIGHT as i32;
    for &(ref msg, color) in game.log.iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
    }

    let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].max_hp(game);
    render_bar(&mut tcod.panel,
               1, 1,
               BAR_WIDTH,
               "HP",
               hp, max_hp,
               colors::LIGHT_RED,
               colors::DARKER_RED
    );
    tcod.panel.print_ex(
        1, 3,
        BackgroundFlag::None,
        TextAlignment::Left,
        format!("Sewers level {}", game.dungeon_level),
    );

    // Draw names under mouse
    tcod.panel.set_default_foreground(colors::LIGHT_GREY);
    tcod.panel.print_ex(1, 0,
                   BackgroundFlag::None,
                   TextAlignment::Left,
                   get_names_under_mouse(tcod.mouse, objects, &tcod.fov)
    );

    blit(&tcod.panel,
         (0, 0),
         (SCREEN_WIDTH, PANEL_HEIGHT),
         &mut tcod.root,
         (0, PANEL_Y),
         1.0,
         1.0
     );

    // Output
    tcod.root.flush();
}

fn initialise_fov(tcod: &mut Tcod, map: &Map) {
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            tcod.fov.set(
                x,
                y,
                !map[x as usize][y as usize].block_sight,
                !map[x as usize][y as usize].blocked
            );
        }
    }
    tcod.con.clear(); // Clear the remnants of the previous games
}

fn new_game(tcod: &mut Tcod) -> (Vec<Object>, Game) {
    let mut player = Object::new("Player", 0, 0, '@', colors::WHITE, true);
    player.alive = true;
    player.fighter = Some(Fighter {
        base_max_hp: 100,
        hp: 100,
        base_defence: 1,
        base_power: 2,
        xp: 0,
        on_death: DeathCallback::Player,
    });

    let mut objects = vec![player];
    let mut game = Game {
        map: make_map(&mut objects, 1),
        log: vec![],
        inventory: vec![],
        dungeon_level: 1,
    };

    let mut dagger = Object::new("dagger", 0, 0, '-', colors::SKY, false);
    dagger.item = Some(Item::Sword);
    dagger.equipment = Some(Equipment {
        equipped: true,
        slot: Slot::LeftHand,
        max_hp_bonus: 0,
        defence_bonus: 0,
        power_bonus: 2,
    });

    game.inventory.push(dagger);

    initialise_fov(tcod, &game.map);

    game.log.add(
        "Welcome stranger! Prepare to perish in the Sewers of the Damned!",
        colors::RED);

    (objects, game)
}

fn play_game(objects: &mut Vec<Object>, game: &mut Game, tcod: &mut Tcod) {
    let mut previous_player_pos = (-1, -1);
    let mut key = Default::default();

    while !tcod.root.window_closed() {
        tcod.con.clear();
        tcod.con.set_default_foreground(colors::WHITE);

        let fov_recompute = previous_player_pos != (objects[PLAYER].x, objects[PLAYER].y);

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => tcod.mouse = m,
            Some((_, Event::Key(k))) => key = k,
            _ => key = Default::default(),
        }

        render_all(tcod, &objects, game, fov_recompute);

        let player = &mut objects[PLAYER];
        previous_player_pos = (player.x, player.y);
        let player_action = handle_keys(key, tcod, objects, game);
        if player_action == PlayerAction::Exit {
            save_game(objects, game).ok().expect("Cannot save");
            break
        }
        level_up(objects, game, tcod);

        if objects[PLAYER].alive && player_action != PlayerAction::DidntTakeTurn {
            for id in 0..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, game, objects, &tcod.fov);
                }
            }
        }
    }
}

fn msgbox(text: &str, width: i32, root: &mut Root) {
    let options: &[&str] = &[];
    menu(text, options, width, root);
}

fn main_menu(tcod: &mut Tcod) {
    let img = tcod::image::Image::from_file("menu_background.png")
        .ok()
        .expect("Background image not found.");
    while !tcod.root.window_closed() {
        // Show the image at twice the resolution
        tcod::image::blit_2x(&img, (0, 0), (-1, -1), &mut tcod.root, (0, 0));
        let choices = &["Play a new game", "Continue last game", "Quit"];
        let choice = menu("", choices, 24, &mut tcod.root);

        match choice {
            Some(0) => {
                let (mut objects, mut game) = new_game(tcod);
                play_game(&mut objects, &mut game, tcod);
            }
            Some(1) => {
                match load_game() {
                    Ok((mut objects, mut game)) => {
                        initialise_fov(tcod, &game.map);
                        play_game(&mut objects, &mut game, tcod);
                    }
                    Err(_e) => {
                        msgbox("\nNo saved game to load.\n", 24, &mut tcod.root);
                        continue;
                    }
                }
            }
            Some(2) => {
                break;
            }
            _ => {}
        }
    }
}

pub fn run_game(font_name: &str, font_layout: FontLayout) -> () {
    let root = Root::initializer()
        .font(font_name, font_layout)
        .font_type(FontType::Default)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("SEWERS OF THE DAMNED")
        .init();
    tcod::system::set_fps(LIMIT_FPS);

    let mut tcod = Tcod {
        root: root,
        con: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        mouse: Default::default(),
    };

    tcod.root.set_default_foreground(colors::LIGHT_YELLOW);
    tcod.root.print_ex(
        SCREEN_WIDTH / 2,
        SCREEN_HEIGHT / 2 - 4,
        BackgroundFlag::None,
        TextAlignment::Center,
        "SEWERS OF THE DAMNED"
    );
    tcod.root.print_ex(
        SCREEN_WIDTH / 2,
        SCREEN_HEIGHT / 2,
        BackgroundFlag::None,
        TextAlignment::Center,
        "By me",
    );

    main_menu(&mut tcod);
}
