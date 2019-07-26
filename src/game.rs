use std::cmp;
use rand::Rng;
use tcod::colors;
use tcod::console::*;
use tcod::input::{self, Key, Event, Mouse};
use tcod::map::{FovAlgorithm, Map as FovMap};

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

#[derive(Clone, Copy, Debug, PartialEq)]
enum Item {
    Heal,
    Lightning,
}

const HEAL_AMOUNT: i32 = 10;
const LIGHTNING_RANGE: i32 = 5;
const LIGHTNING_DAMAGE: i32 = 20;

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
    item: Option<Item>,
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
            item: None,
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
        if x < 0 || y < 0 || x > MAP_WIDTH || x > MAP_HEIGHT {
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

    pub fn take_damage(&mut self, damage: i32, messages: &mut Messages) {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }
        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self, messages);
            }
        }
    }

    pub fn attack(&mut self, target: &mut Object, messages: &mut Messages) {
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defence);
        if damage > 0 {
            message(messages,
                    format!("{} swings and hits {} for {} damage!", self.name, target.name, damage),
                    colors::WHITE);
            target.take_damage(damage, messages);
        } else {
            message(messages,
                    format!("{} attacks the {} but it has no effect!", self.name, target.name),
                    colors::WHITE);
        }
    }

    pub fn heal(&mut self, amount: i32) {
        if let Some(ref mut fighter) = self.fighter {
            fighter.hp += amount;
            if fighter.hp > fighter.max_hp {
                fighter.hp = fighter.max_hp;
            }
        }
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
    message(messages, "You die!", colors::RED);

    player.char = '%';
    player.color = colors::DARK_RED;
}

fn monster_death(monster: &mut Object, messages: &mut Messages) {
    message(messages, format!("{} dies!", monster.name), colors::ORANGE);
    monster.char = '%';
    monster.color = colors::DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("Remains of {}", monster.name);
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    if x < 0 || y < 0 {
        return false;
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

fn ai_take_turn(monster_id: usize, map: &Map, objects: &mut [Object], fov_map: &FovMap, messages: &mut Messages) {
    let (monster_x, monster_y) = objects[monster_id].pos();
    if fov_map.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, map, objects);
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp > 0) {
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.attack(player, messages);
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
        move_towards(monster_id, tx, ty, map, objects);
    }
}

type Messages = Vec<(String, colors::Color)>;

fn message<T: Into<String>>(messages: &mut Messages, message: T, color: colors::Color) {
    if messages.len() == MSG_HEIGHT {
        messages.remove(0);
    }
    messages.push((message.into(), color));
}

//////////////////////// MAPGEN
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;
const MAX_ROOM_MONSTERS: i32 = 3;
const MAX_ROOM_ITEMS: i32 = 2;

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

    let num_items = rand::thread_rng().gen_range(0, MAX_ROOM_ITEMS + 1);

    for _ in 0..num_items {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);
        if !is_blocked(x, y, map, objects) {
            let dice = rand::random::<f32>();
            let item = if dice < 0.7 {
                let mut object = Object::new("healing potion", x, y, 
                                             '!', colors::VIOLET, false);
                object.item = Some(Item::Heal);
                object
            } else {
                let mut object = Object::new("scroll of lightning", x, y,
                                             '#', colors::LIGHT_YELLOW, false);
                object.item = Some(Item::Lightning);
                object
            };
            objects.push(item);
        }
    }
}

fn pick_item_up(
    object_id: usize,
    objects: &mut Vec<Object>,
    inventory: &mut Vec<Object>,
    messages: &mut Messages,
) {
    if inventory.len() >= 26 {
        message(messages,
                format!("Your inventory is full, cannot pick up {}.",
                        objects[object_id].name),
                colors::RED);
    } else {
        let item = objects.swap_remove(object_id);
        message(messages,
                format!("You picked up a {}!", item.name),
                colors::GREEN);
        inventory.push(item);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

fn player_move_or_attack(dx: i32, dy: i32, map: &Map, objects: &mut [Object], messages: &mut Messages) {
    let x = objects[PLAYER].x + dx;
    let y = objects[PLAYER].y + dy;

    let target_id = objects.iter().position(|object| object.fighter.is_some() && object.pos() == (x, y));
    match target_id {
        Some(target_id) => {
            let (player, monster) = mut_two(PLAYER, target_id, objects);
            player.attack(monster, messages);
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
}

fn cast_heal(
    _inventory_id: usize,
    objects: &mut [Object],
    messages: &mut Messages,
    tcod: &mut Tcod
) -> UseResult {
    if let Some(fighter) = objects[PLAYER].fighter {
        if fighter.hp == fighter.max_hp {
            message(messages, "You are already at full health.", colors::RED);
            return UseResult::Cancelled;
        }
        message(messages, "Your wounds are healing!", colors::LIGHT_VIOLET);
        objects[PLAYER].heal(HEAL_AMOUNT);
        return UseResult::UsedUp;
    }
    UseResult::Cancelled
}

fn cast_lightning(
    _inventory_id: usize,
    objects: &mut [Object],
    messages: &mut Messages,
    tcod: &mut Tcod,
) -> UseResult {
    let monster_id = closest_monster(LIGHTNING_RANGE, objects, tcod);
    if let Some(monster_id) = monster_id {
        message(messages,
                format!("A lightning strikes {} with a loud thunder for {} damage!",
                        objects[monster_id].name, LIGHTNING_DAMAGE),
                colors::LIGHT_BLUE,
        );
        objects[monster_id].take_damage(LIGHTNING_DAMAGE, messages);
        UseResult::UsedUp
    } else {
        message(messages, "No enemy is close enough to strike.", colors::RED);
        UseResult::Cancelled
    }
}

fn use_item(
    inventory_id: usize,
    inventory: &mut Vec<Object>,
    objects: &mut [Object],
    messages: &mut Messages,
    tcod: &mut Tcod,
) {
    use Item::*;
    if let Some(item) = inventory[inventory_id].item {
        let on_use = match item {
            Heal => cast_heal,
            Lightning => cast_lightning,
        };
        match on_use(inventory_id, objects, messages, tcod) {
            UseResult::UsedUp => {
                // destroy after use
                inventory.remove(inventory_id);
            }
            UseResult::Cancelled => {
                message(messages, "Cancelled", colors::WHITE);
            }
        }
    } else {
        message(messages,
                format!("The {} cannot be used.", inventory[inventory_id].name),
                colors::WHITE);
    }
}

///////////////////////////////// UI Work
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;
const COLOR_DARK_WALL: colors::Color = colors::Color    { r: 0,     g: 0,   b: 100 };
const COLOR_LIGHT_WALL: colors::Color = colors::Color   { r: 130,   g: 110, b: 50 };
const COLOR_DARK_GROUND: colors::Color = colors::Color  { r: 50,    g: 50,  b: 150 };
const COLOR_LIGHT_GROUND: colors::Color = colors::Color { r: 200,   g: 180, b: 50 };
const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;
const INVENTORY_WIDTH: i32 = 50;

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
               map: &Map,
               messages: &mut Messages,
               inventory: &mut Vec<Object>,
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
        (Key { code: Up, .. }, true) => {
            player_move_or_attack(0, -1, map, objects, messages);
            TookTurn
        }
        (Key { code: Down, .. }, true) => {
            player_move_or_attack(0, 1, map, objects, messages);
            TookTurn
        }
        (Key { code: Left, .. }, true) => {
            player_move_or_attack(-1, 0, map, objects, messages);
            TookTurn
        }
        (Key { code: Right, .. }, true) => {
            player_move_or_attack(1, 0, map, objects, messages);
            TookTurn
        }
        (Key {printable: 'g', .. }, true) => {
            let item_id = objects
                .iter()
                .position(|object| object.pos() == objects[PLAYER].pos() &&
                          object.item.is_some());
            if let Some(item_id) = item_id {
                pick_item_up(item_id, objects, inventory, messages);
            }
            DidntTakeTurn
        }
        (Key {printable: 'i', .. }, true) => {
            let inventory_index = inventory_menu(
                inventory,
                "Press the key next to an item to use it, any other to cancel.\n",
                &mut tcod.root);
            if let Some(inventory_index) = inventory_index {
                use_item(inventory_index, inventory, objects, messages, tcod);
            }
            DidntTakeTurn
        }
        _ => DidntTakeTurn,
    };
    action
}

fn menu<T: AsRef<str>>(header: &str, options: &[T], width: i32, root: &mut Root) -> Option<usize> {
    assert!(options.len() <= 26, "Menu can only fit 26 options");

    let header_height = root.get_height_rect(0, 0, width, SCREEN_HEIGHT, header);
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
        inventory.iter().map(|item| { item.name.clone() }).collect()
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

fn render_all(tcod: &mut Tcod,
              objects: &[Object],
              map: &mut Map,
              messages: &Messages,
              fov_recompute: bool,
) {
    if fov_recompute {
        let player = &objects[PLAYER];
        tcod.fov.compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
    }
    // Draw map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let visible = tcod.fov.is_in_fov(x, y);
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
                tcod.con.set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }
    }
    // Draw objects
    let mut to_draw: Vec<_> = objects.
        iter().
        filter(|obj| tcod.fov.is_in_fov(obj.x, obj.y)).
        collect();
    to_draw.sort_by(|o1, o2| { o1.blocks.cmp(&o2.blocks) });
    for object in to_draw {
        if tcod.fov.is_in_fov(object.x, object.y) {
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
    for &(ref msg, color) in messages.iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
    }

    let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].fighter.map_or(0, |f| f.max_hp);
    render_bar(&mut tcod.panel,
               1, 1,
               BAR_WIDTH,
               "HP",
               hp, max_hp,
               colors::LIGHT_RED,
               colors::DARKER_RED
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
pub fn run_game(font_name: &str, font_layout: FontLayout) -> () {
    let root = Root::initializer()
        .font(font_name, font_layout)
        .font_type(FontType::Default)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Roguelikedev tutorial in Rust")
        .init();
    tcod::system::set_fps(LIMIT_FPS);

    let mut tcod = Tcod {
        root: root,
        con: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        mouse: Default::default(),
    };

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

    let mut inventory = vec![];

    let mut previous_player_pos = (-1, -1);
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

    let mut key = Default::default();

    let mut messages = vec![];
    message(&mut messages,
            "Welcome stranger! Prepare to perish in the Sewers of the Damned!",
            colors::RED);

    while !tcod.root.window_closed() {
        tcod.con.clear();
        tcod.con.set_default_foreground(colors::WHITE);

        let fov_recompute = previous_player_pos != (objects[PLAYER].x, objects[PLAYER].y);

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => tcod.mouse = m,
            Some((_, Event::Key(k))) => key = k,
            _ => key = Default::default(),
        }

        render_all(&mut tcod,
                   &objects,
                   &mut map,
                   &messages,
                   fov_recompute,
        );

        let player = &mut objects[PLAYER];
        previous_player_pos = (player.x, player.y);
        let player_action = handle_keys(key,
                                        &mut tcod,
                                        &mut objects,
                                        &map,
                                        &mut messages,
                                        &mut inventory);
        if player_action == PlayerAction::Exit {
            break
        }

        if objects[PLAYER].alive && player_action != PlayerAction::DidntTakeTurn {
            for id in 0..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, &map, &mut objects, &tcod.fov, &mut messages);
                }
            }
        }
    }
}
