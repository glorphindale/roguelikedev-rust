use tcod::colors::*;
use tcod::console::*;

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const LIMIT_FPS: i32 = 60;

struct Object {
    x: i32,
    y: i32,
    char: char,
    color: Color,
}

impl Object {
    pub fn new(x: i32, y: i32, char: char, color: Color) -> Self {
        Object { x, y, char, color}
    }
    pub fn move_by(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }
    pub fn draw(&self, con: &mut Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }
}

fn handle_keys(root: &mut Root, player: &mut Object) -> bool {
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
        Key { code: Up, .. } => player.move_by(0, -1),
        Key { code: Down, .. } => player.move_by(0, 1),
        Key { code: Left, .. } => player.move_by(-1, 0),
        Key { code: Right, .. } => player.move_by(1, 0),
        _ => {},
    }
    false
}

fn main() {
    let mut root = Root::initializer()
        .font("Cheepicus_15x15.png", FontLayout::AsciiInRow)
        .font_type(FontType::Default)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Roguelikedev tutorial in Rust")
        .init();
    tcod::system::set_fps(LIMIT_FPS);

    let mut con = Offscreen::new(SCREEN_WIDTH, SCREEN_HEIGHT);

    let player = Object::new(SCREEN_WIDTH / 2, SCREEN_HEIGHT / 2, '@', WHITE);
    let npc = Object::new(SCREEN_WIDTH / 2 - 2, SCREEN_HEIGHT /2, '@', YELLOW);
    let mut objects = [player, npc];

    while !root.window_closed() {
        con.clear();
        con.set_default_foreground(WHITE);

        for object in &objects {
            object.draw(&mut con);
        }

        blit(&mut con, (0, 0), (SCREEN_WIDTH, SCREEN_HEIGHT),
             &mut root, (0, 0),
             1.0,
             1.0);
        root.flush();

        let player = &mut objects[0];
        let exit = handle_keys(&mut root, player);
        if exit {
            break
        }
    }
}
