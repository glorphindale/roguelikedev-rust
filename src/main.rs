use tcod::colors::*;
use tcod::console::*;

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 45;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_DARK_GROUND: Color = Color { r: 50, g: 50, b: 150 };

const LIMIT_FPS: i32 = 60;

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
    block_sight: bool
}

impl Tile {
    pub fn empty() -> Self {
        Tile{ blocked: false, block_sight: false }
    }
    pub fn wall() -> Self {
        Tile{ blocked: true, block_sight: true }
    }
}

type Map = Vec<Vec<Tile>>;

fn make_map() -> Map {
    let mut map = vec![vec![Tile::empty(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    map[30][22] = Tile::wall();
    map[50][22] = Tile::wall();
    map
}

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

fn render_all(root: &mut Root, con: &mut Offscreen, objects: &[Object], map: &Map) {
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let wall = map[x as usize][y as usize].block_sight;
            if wall {
                con.set_char_background(x, y, COLOR_DARK_WALL, BackgroundFlag::Set);
            } else {
                con.set_char_background(x, y, COLOR_DARK_GROUND, BackgroundFlag::Set);
            }
        }
    }
    for object in objects {
        object.draw(con);
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

    let map = make_map();
    let player = Object::new(SCREEN_WIDTH / 2, SCREEN_HEIGHT / 2, '@', WHITE);
    let npc = Object::new(SCREEN_WIDTH / 2 - 2, SCREEN_HEIGHT /2, '@', YELLOW);
    let mut objects = [player, npc];

    while !root.window_closed() {
        con.clear();
        con.set_default_foreground(WHITE);

        render_all(&mut root, &mut con, &objects, &map);

        let player = &mut objects[0];
        let exit = handle_keys(&mut root, player, &map);
        if exit {
            break
        }
    }
}
