use std::cmp;
use rand::Rng;
use tcod::colors::*;
use tcod::console::*;
use tcod::map::{FovAlgorithm, Map as FovMap};

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 45;

#[derive(Debug)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: Color,
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

#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32, y1: i32,
    x2: i32, y2: i32,
}

//////////////////////// MAPGEN
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;

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

fn create_room(room: Rect, map: &mut Map) {
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
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

fn make_map() -> (Map, (i32, i32)) {
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
            create_room(new_room, &mut map);
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

impl Object {
    pub fn new(x: i32, y: i32, char: char, color: Color) -> Self {
        Object { x, y, char, color}
    }
    pub fn move_by(&mut self, dx: i32, dy: i32, map: &Map) {
        let nx = self.x + dx;
        let ny = self.y + dy;
        if !map[nx as usize][ny as usize].blocked {
            self.x = nx;
            self.y = ny;
        }
    }
    pub fn draw(&self, con: &mut Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }
}

///////////////////////////////// UI Work
const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;
const COLOR_DARK_WALL: Color = Color    { r: 0,     g: 0,   b: 100 };
const COLOR_LIGHT_WALL: Color = Color   { r: 130,   g: 110, b: 50 };
const COLOR_DARK_GROUND: Color = Color  { r: 50,    g: 50,  b: 150 };
const COLOR_LIGHT_GROUND: Color = Color { r: 200,   g: 180, b: 50 };

const LIMIT_FPS: i32 = 60;

fn handle_keys(root: &mut Root, player: &mut Object, map: &Map) -> bool {
    use tcod::input::Key;
    use tcod::input::KeyCode::*;

    let key = root.wait_for_keypress(true);
    match key {
        // TODO this does not work properly on Linux Mint :(
        /* Key { code: Enter, alt: true, .. } => {
            let fullscreen = root.is_fullscreen();
            root.set_fullscreen(!fullscreen);
        } */
        Key { code: Escape, .. } => return true,
        Key { code: Up, .. } => player.move_by(0, -1, map),
        Key { code: Down, .. } => player.move_by(0, 1, map),
        Key { code: Left, .. } => player.move_by(-1, 0, map),
        Key { code: Right, .. } => player.move_by(1, 0, map),
        _ => {},
    }
    false
}

fn render_all(root: &mut Root, con: &mut Offscreen, objects: &[Object], map: &mut Map, fov_map: &mut FovMap, fov_recompute: bool) {
    if fov_recompute {
        let player = &objects[0];
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
    for object in objects {
        if fov_map.is_in_fov(object.x, object.y) {
            object.draw(con);
        }
    }

    blit(con, (0, 0), (MAP_WIDTH, MAP_HEIGHT),
         root, (0, 0),
         1.0,
         1.0);
    root.flush();
}

fn main() {
    let mut root = Root::initializer()
        .font("Cheepicus_15x15.png", FontLayout::AsciiInRow)
        .font_type(FontType::Default)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Roguelikedev tutorial in Rust")
        .init();
    tcod::system::set_fps(LIMIT_FPS);

    let mut con = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);

    let (mut map, (px, py)) = make_map();
    let player = Object::new(px, py, '@', WHITE);
    let mut objects = [player];

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
        con.set_default_foreground(WHITE);

        let fov_recompute = previous_player_pos != (objects[0].x, objects[0].y);

        render_all(&mut root, &mut con, &objects, &mut map, &mut fov_map, fov_recompute);

        let player = &mut objects[0];
        previous_player_pos = (player.x, player.y);
        let exit = handle_keys(&mut root, player, &map);
        if exit {
            break
        }

    }
}
