use std::cmp;
use rand::Rng;
use tcod::colors;
use tcod::console::*;
use tcod::map::{FovAlgorithm, Map as FovMap};

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 45;
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

#[derive(Clone, Copy, Debug)]
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
}

type Map = Vec<Vec<Tile>>;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defence: i32,
    power: i32,
    on_death: DeathCallback,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AI;

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeathCallback {
    Player,
    Monster,
}

#[derive(Debug)]
struct Object {
    name: String,
    x: i32,
    y: i32,
    char: char,
    color: colors::Color,
    blocks: bool,
    alive: bool,
    fighter: Option<Fighter>,
    ai: Option<AI>,
}

impl Object {
    pub fn new(name: &str, x: i32, y: i32, char: char, color: colors::Color, blocks: bool) -> Self {
        Object {
            name: name.into(),
            x: x, y: y,
            char: char,
            color: color,
            blocks: blocks,
            alive: false,
            fighter: None,
            ai: None,
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
        self.x = x;
        self.y = y;
    }

    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    pub fn take_damage(&mut self, damage: i32) {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }
        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self);
            }
        }
    }

    pub fn attack(&mut self, target: &mut Object) {
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defence);
        if damage > 0 {
            println!("{} swings and hits {} for {} damage!", self.name, target.name, damage);
            target.take_damage(damage);
        } else {
            println!("{} attacks the {} but it has no effect!", self.name, target.name);
        }
    }
}

impl DeathCallback {
    fn callback(self, object: &mut Object) {
        use DeathCallback::*;
        let callback: fn(&mut Object) = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(object);
    }
}

fn player_death(player: &mut Object) {
    println!("You die!");

    player.char = '%';
    player.color = colors::DARK_RED;
}

fn monster_death(monster: &mut Object) {
    println!("{} dies!", monster.name);
    monster.char = '%';
    monster.color = colors::DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("Remains of {}", monster.name);
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
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

fn ai_take_turn(monster_id: usize, map: &Map, objects: &mut [Object], fov_map: &FovMap) {
    let (monster_x, monster_y) = objects[monster_id].pos();
    if fov_map.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, map, objects);
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.attack(player);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32, y1: i32,
    x2: i32, y2: i32,
}

//////////////////////// MAPGEN
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;
const MAX_ROOM_MONSTERS: i32 = 3;

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

fn create_room(room: Rect, map: &mut Map, objects: &mut Vec<Object>) {
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
    place_objects(room, objects, map)
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

fn make_map(objects: &mut Vec<Object>) -> (Map, (i32, i32)) {
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    let mut rooms = vec![];
    let mut starting_position = (0, 0);
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
            create_room(new_room, &mut map, objects);
            let (new_x, new_y) = new_room.center();
            if rooms.is_empty() {
                starting_position = (new_x, new_y);
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
    (map, starting_position)
}

/////////////////////// Logic
const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = true;
const TORCH_RADIUS: i32 = 10;

fn place_objects(room: Rect, objects: &mut Vec<Object>, map: &Map) {
    let num_monsters = rand::thread_rng().gen_range(0, MAX_ROOM_MONSTERS + 1);

    for _ in 0..num_monsters {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);
        if is_blocked(x, y, map, objects) {
            continue;
        }
        let mut monster = if rand::random::<f32>() < 0.8 {
            let mut orc = Object::new("Orc", x, y, '0', colors::DESATURATED_GREEN, true);
            orc.fighter = Some(Fighter {
                max_hp: 10,
                hp: 10,
                defence: 0,
                power: 3,
                on_death: DeathCallback::Monster,
            });
            orc.ai = Some(AI);
            orc
        } else {
            let mut troll = Object::new("Troll", x, y, 'T', colors::DARKER_GREEN, true);
            troll.fighter = Some(Fighter {
                max_hp: 16,
                hp: 16,
                defence: 1,
                power: 4,
                on_death: DeathCallback::Monster,
            });
            troll.ai = Some(AI);
            troll
        };
        monster.alive = true;
        objects.push(monster);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

fn player_move_or_attack(dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let x = objects[PLAYER].x + dx;
    let y = objects[PLAYER].y + dy;

    let target_id = objects.iter().position(|object| object.fighter.is_some() && object.pos() == (x, y));
    match target_id {
        Some(target_id) => {
            let (player, monster) = mut_two(PLAYER, target_id, objects);
            player.attack(monster);
        }
        None => {
            move_by(PLAYER, dx, dy, map, objects)
        }
    }
}

fn move_towards(id: usize, target_x: i32, target_y: i32, map: &Map, objects: &mut [Object]) {
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_by(id, dx, dy, map, objects);
}

///////////////////////////////// UI Work
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;
const COLOR_DARK_WALL: colors::Color = colors::Color    { r: 0,     g: 0,   b: 100 };
const COLOR_LIGHT_WALL: colors::Color = colors::Color   { r: 130,   g: 110, b: 50 };
const COLOR_DARK_GROUND: colors::Color = colors::Color  { r: 50,    g: 50,  b: 150 };
const COLOR_LIGHT_GROUND: colors::Color = colors::Color { r: 200,   g: 180, b: 50 };

const LIMIT_FPS: i32 = 60;

fn handle_keys(root: &mut Root, objects: &mut [Object], map: &Map) -> PlayerAction {
    use PlayerAction::*;
    use tcod::input::Key;
    use tcod::input::KeyCode::*;

    let key = root.wait_for_keypress(true);
    let alive = objects[0].alive;
    let action = match (key, alive) {
        (Key { code: Enter, alt: true, .. }, _) => {
            let fullscreen = root.is_fullscreen();
            root.set_fullscreen(!fullscreen);
            DidntTakeTurn
        }
        (Key { code: Escape, .. }, _) => return Exit,
        (Key { code: Up, .. }, true) => {
            player_move_or_attack(0, -1, map, objects);
            TookTurn
        }
        (Key { code: Down, .. }, true) => {
            player_move_or_attack(0, 1, map, objects);
            TookTurn
        }
        (Key { code: Left, .. }, true) => {
            player_move_or_attack(-1, 0, map, objects);
            TookTurn
        }
        (Key { code: Right, .. }, true) => {
            player_move_or_attack(1, 0, map, objects);
            TookTurn
        }
        _ => DidntTakeTurn,
    };
    action
}

fn render_all(root: &mut Root, con: &mut Offscreen, objects: &[Object], map: &mut Map, fov_map: &mut FovMap, fov_recompute: bool) {
    if fov_recompute {
        let player = &objects[PLAYER];
        fov_map.compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
    }
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let visible = fov_map.is_in_fov(x, y);
            let wall = map[x as usize][y as usize].block_sight;
            let color = match (visible, wall) {
                // Outside of FOV
                (false, true) => COLOR_DARK_WALL,
                (false, false) => COLOR_DARK_GROUND,
                // Inside the FOV
                (true, false) => COLOR_LIGHT_GROUND,
                (true, true) => COLOR_LIGHT_WALL,
            };
            let explored = &mut map[x as usize][y as usize].explored;
            if visible {
                *explored = true;
            }
            if *explored {
                con.set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }
    }
    let mut to_draw: Vec<_> = objects.
        iter().
        filter(|obj| fov_map.is_in_fov(obj.x, obj.y)).
        collect();
    to_draw.sort_by(|o1, o2| { o1.blocks.cmp(&o2.blocks) });
    for object in to_draw {
        if fov_map.is_in_fov(object.x, object.y) {
            object.draw(con);
        }
    }

    if let Some(fighter) = &objects[PLAYER].fighter {
        root.print_ex(1, SCREEN_HEIGHT - 2,
                      BackgroundFlag::None,
                      TextAlignment::Left,
                      format!("HP: {}/{}", fighter.hp, fighter.max_hp));
    }

    blit(con, (0, 0), (MAP_WIDTH, MAP_HEIGHT),
         root, (0, 0),
         1.0,
         1.0);
    root.flush();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let args_len = args.len();
    let (font_name, font_layout) = match args_len {
        2 => (args[1].to_string(), FontLayout::AsciiInRow),
        _ => ("courier12x12_aa_tc.png".to_string(), FontLayout::Tcod)
    };

    let mut root = Root::initializer()
        .font(font_name, font_layout)
        .font_type(FontType::Default)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Roguelikedev tutorial in Rust")
        .init();
    tcod::system::set_fps(LIMIT_FPS);

    let mut con = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);

    let mut player = Object::new("Player", 0, 0, '@', colors::WHITE, true);
    player.alive = true;
    player.fighter = Some(Fighter {
        max_hp: 30,
        hp: 30,
        defence: 2,
        power: 5,
        on_death: DeathCallback::Player,
    });

    let mut objects = vec![player];
    let (mut map, (px, py)) = make_map(&mut objects);
    objects[PLAYER].set_pos(px, py);

    let mut previous_player_pos = (-1, -1);
    let mut fov_map = FovMap::new(MAP_WIDTH, MAP_HEIGHT);
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            fov_map.set(
                x,
                y,
                !map[x as usize][y as usize].block_sight,
                !map[x as usize][y as usize].blocked
            );
        }
    }
    while !root.window_closed() {
        con.clear();
        con.set_default_foreground(colors::WHITE);

        let fov_recompute = previous_player_pos != (objects[PLAYER].x, objects[PLAYER].y);

        render_all(&mut root, &mut con, &objects, &mut map, &mut fov_map, fov_recompute);

        let player = &mut objects[PLAYER];
        previous_player_pos = (player.x, player.y);
        let player_action = handle_keys(&mut root, &mut objects, &map);
        if player_action == PlayerAction::Exit {
            break
        }

        if objects[PLAYER].alive && player_action != PlayerAction::DidntTakeTurn {
            for id in 0..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, &map, &mut objects, &fov_map);
                }
            }
        }
    }
}
