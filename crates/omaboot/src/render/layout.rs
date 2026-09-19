//! Where things sit on the screen.
//!
//! These numbers are the single source of truth for both renders of a theme:
//! the Plymouth script gets them injected as data, and the composited preview
//! computes with them directly. Neither side carries a literal of its own, so
//! the picture omaboot draws and the picture Plymouth draws cannot drift apart
//! by someone changing one and forgetting the other.
//!
//! The arithmetic mirrors what the Plymouth script language does: integer
//! division truncates, so the Rust side truncates too.

use crate::theme::{Manifest, Position};

/// Distance between the bottom of the logo and the top of the entry.
pub const LOGO_ENTRY_GAP: i64 = 40;
/// Distance between the lock glyph and the entry.
pub const LOCK_ENTRY_GAP: i64 = 15;
/// The lock glyph is drawn slightly shorter than the entry.
pub const LOCK_HEIGHT_RATIO: f64 = 0.8;
/// Bullets are square.
pub const BULLET_SIZE: i64 = 7;
/// Distance from one bullet's left edge to the next.
pub const BULLET_PITCH: i64 = 12;
/// How far the first bullet, and any typed text, sits inside the entry.
pub const ENTRY_INSET: i64 = 20;
/// Distance between the entry and the theme's message.
pub const MESSAGE_GAP: i64 = 40;
/// Upstream stops drawing bullets here, and so does omaboot.
pub const MAX_BULLETS: i64 = 21;
/// With `position = "top"`, the logo sits this fraction down the screen.
pub const TOP_POSITION_DIVISOR: i64 = 8;
/// The side margin of a left or right aligned login panel, in percent.
pub const LOGIN_SIDE_MARGIN_PERCENT: i64 = 12;
/// The margin between the clock and the corner of the greeter.
pub const CLOCK_MARGIN: i64 = 40;
/// The font Plymouth draws messages and typed text with, as the `.plymouth`
/// file names it. Plymouth lays text out at 96 dpi, so 11 points is
/// `PLYMOUTH_FONT_PX` pixels, which is what the composite draws with.
pub const PLYMOUTH_FONT: &str = "Cantarell 11";
pub const PLYMOUTH_FONT_FAMILY: &str = "Cantarell";
pub const PLYMOUTH_FONT_PX: u32 = 15;
/// The greeter's clock, as `Main.qml` sets `font.pixelSize`.
pub const LOGIN_CLOCK_PX: u32 = 20;
pub const LOGIN_FONT_FAMILY: &str = "JetBrainsMono Nerd Font";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

impl Rect {
    pub fn new(x: i64, y: i64, width: i64, height: i64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn centre_x(&self) -> i64 {
        self.x + self.width / 2
    }

    pub fn bottom(&self) -> i64 {
        self.y + self.height
    }
}

/// The natural size of each image the layout places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sizes {
    pub logo: (i64, i64),
    pub entry: (i64, i64),
    pub lock: (i64, i64),
    pub progress_box: (i64, i64),
    pub progress_bar: (i64, i64),
}

/// The screen being drawn. The three tabs of the TUI, and the three things a
/// user actually sees around a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Unlock,
    Login,
    Shutdown,
}

impl Screen {
    pub fn title(self) -> &'static str {
        match self {
            Self::Unlock => "Unlock",
            Self::Login => "Login",
            Self::Shutdown => "Shutdown",
        }
    }

    /// Plymouth draws the unlock and shutdown screens; SDDM draws the login.
    pub fn is_plymouth(self) -> bool {
        matches!(self, Self::Unlock | Self::Shutdown)
    }
}

/// Everything the unlock and shutdown screens place, for one screen size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlymouthLayout {
    pub logo: Rect,
    pub entry: Rect,
    pub lock: Rect,
    pub progress_box: Rect,
    pub progress_bar: Rect,
    /// Where a bullet goes, left to right, for a given number of characters.
    pub bullet_origin: (i64, i64),
    /// The baseline anchor of the theme's message, centred on this x.
    pub message: (i64, i64),
}

/// Compute the unlock or shutdown layout, exactly as the generated script does.
pub fn plymouth(
    width: i64,
    height: i64,
    sizes: &Sizes,
    manifest: &Manifest,
    screen: Screen,
) -> PlymouthLayout {
    let (logo_w, logo_h) = sizes.logo;
    let (entry_w, entry_h) = sizes.entry;
    let (lock_image_w, lock_image_h) = sizes.lock;
    let placement = manifest.placement(screen);

    let mut logo_x = width / 2 - logo_w / 2;
    let mut logo_y = match placement.position {
        Position::Top => height / TOP_POSITION_DIVISOR,
        Position::Center | Position::Custom => height / 2 - logo_h / 2,
    };
    logo_x += placement.offset[0];
    logo_y += placement.offset[1];

    let entry_x = width / 2 - entry_w / 2;
    let entry_y = logo_y + logo_h + LOGO_ENTRY_GAP;

    let lock_h = (entry_h as f64 * LOCK_HEIGHT_RATIO) as i64;
    let lock_scale = if lock_image_h == 0 {
        0.0
    } else {
        lock_h as f64 / lock_image_h as f64
    };
    let lock_w = (lock_image_w as f64 * lock_scale) as i64;

    let (box_w, box_h) = sizes.progress_box;
    let (bar_w, bar_h) = sizes.progress_bar;

    PlymouthLayout {
        logo: Rect::new(logo_x, logo_y, logo_w, logo_h),
        entry: Rect::new(entry_x, entry_y, entry_w, entry_h),
        lock: Rect::new(
            entry_x - lock_w - LOCK_ENTRY_GAP,
            entry_y + entry_h / 2 - lock_h / 2,
            lock_w,
            lock_h,
        ),
        progress_box: Rect::new(
            width / 2 - box_w / 2,
            entry_y + entry_h / 2 - box_h / 2,
            box_w,
            box_h,
        ),
        progress_bar: Rect::new(
            width / 2 - bar_w / 2,
            entry_y + entry_h / 2 - bar_h / 2,
            bar_w,
            bar_h,
        ),
        bullet_origin: (
            entry_x + ENTRY_INSET,
            entry_y + entry_h / 2 - BULLET_SIZE / 2,
        ),
        message: (width / 2, entry_y + entry_h + MESSAGE_GAP),
    }
}

/// The nth bullet's position, given the origin.
pub fn bullet(origin: (i64, i64), index: i64) -> Rect {
    Rect::new(
        origin.0 + index * BULLET_PITCH,
        origin.1,
        BULLET_SIZE,
        BULLET_SIZE,
    )
}

/// Everything the login screen places. The greeter is QML, which lays itself
/// out at runtime, so this mirrors the generated `Main.qml` rather than
/// driving it: a centred column of the logo and a row of lock plus entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginLayout {
    pub logo: Rect,
    pub entry: Rect,
    pub lock: Rect,
    pub bullet_origin: (i64, i64),
    pub clock: (i64, i64),
}

pub fn login(width: i64, height: i64, sizes: &Sizes, manifest: &Manifest) -> LoginLayout {
    use crate::theme::LoginLayout as Align;

    // `sizes.logo` is already the share of the screen width the theme asks
    // for, as `Main.qml` computes it from `root.width`.
    let (logo_w, logo_h) = sizes.logo;
    let (entry_w, entry_h) = sizes.entry;
    // The greeter draws the lock at a fixed size, which the QML states.
    let (lock_w, lock_h) = (34, 38);

    let row_h = entry_h.max(lock_h);
    let column_h = logo_h + LOGO_ENTRY_GAP + row_h;
    let row_w = lock_w + LOCK_ENTRY_GAP + entry_w;
    let column_w = logo_w.max(row_w);

    let column_x = match manifest.login.layout {
        Align::Centered => width / 2 - column_w / 2,
        Align::Left => width * LOGIN_SIDE_MARGIN_PERCENT / 100,
        Align::Right => width - width * LOGIN_SIDE_MARGIN_PERCENT / 100 - column_w,
    };
    let column_y = height / 2 - column_h / 2;
    let centre_x = column_x + column_w / 2;

    let row_y = column_y + logo_h + LOGO_ENTRY_GAP;
    let row_x = centre_x - row_w / 2;
    let entry_x = row_x + lock_w + LOCK_ENTRY_GAP;

    LoginLayout {
        logo: Rect::new(centre_x - logo_w / 2, column_y, logo_w, logo_h),
        entry: Rect::new(entry_x, row_y + row_h / 2 - entry_h / 2, entry_w, entry_h),
        lock: Rect::new(row_x, row_y + row_h / 2 - lock_h / 2, lock_w, lock_h),
        bullet_origin: (entry_x + ENTRY_INSET, row_y + row_h / 2 - BULLET_SIZE / 2),
        clock: (width - CLOCK_MARGIN, CLOCK_MARGIN),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes() -> Sizes {
        Sizes {
            logo: (300, 100),
            entry: (400, 50),
            lock: (84, 96),
            progress_box: (400, 20),
            progress_bar: (396, 16),
        }
    }

    fn manifest(body: &str) -> Manifest {
        toml::from_str(body).unwrap()
    }

    #[test]
    fn a_centred_logo_is_centred_and_the_entry_hangs_below_it() {
        let layout = plymouth(
            1920,
            1080,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n"),
            Screen::Unlock,
        );
        assert_eq!(layout.logo, Rect::new(810, 490, 300, 100));
        // entry_y = logo_y + logo_h + 40
        assert_eq!(layout.entry, Rect::new(760, 630, 400, 50));
        assert_eq!(layout.entry.centre_x(), 960);
    }

    #[test]
    fn the_lock_is_shorter_than_the_entry_and_sits_to_its_left() {
        let layout = plymouth(
            1920,
            1080,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n"),
            Screen::Unlock,
        );
        assert_eq!(layout.lock.height, 40); // 50 * 0.8
        assert_eq!(layout.lock.width, 35); // 84 * (40/96)
        assert_eq!(layout.lock.x, 760 - 35 - LOCK_ENTRY_GAP);
        // Vertically centred on the entry.
        assert_eq!(
            layout.lock.y + layout.lock.height / 2,
            layout.entry.y + layout.entry.height / 2
        );
    }

    #[test]
    fn bullets_march_right_from_inside_the_entry() {
        let layout = plymouth(
            1920,
            1080,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n"),
            Screen::Unlock,
        );
        let first = bullet(layout.bullet_origin, 0);
        let second = bullet(layout.bullet_origin, 1);
        assert_eq!(first.x, layout.entry.x + ENTRY_INSET);
        assert_eq!(second.x - first.x, BULLET_PITCH);
        assert_eq!(first.width, BULLET_SIZE);
    }

    #[test]
    fn a_top_logo_sits_an_eighth_down_the_screen() {
        let layout = plymouth(
            1920,
            1080,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n[logo]\nposition = \"top\"\n"),
            Screen::Unlock,
        );
        assert_eq!(layout.logo.y, 135);
    }

    #[test]
    fn a_custom_offset_moves_the_logo_and_everything_under_it() {
        let centred = plymouth(
            1920,
            1080,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n"),
            Screen::Unlock,
        );
        let offset = plymouth(
            1920,
            1080,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n[logo]\nposition = \"custom\"\noffset = [10, -40]\n"),
            Screen::Unlock,
        );
        assert_eq!(offset.logo.x - centred.logo.x, 10);
        assert_eq!(offset.logo.y - centred.logo.y, -40);
        // The entry follows the logo, as it does in the script.
        assert_eq!(offset.entry.y - centred.entry.y, -40);
        // But it stays horizontally centred on the screen.
        assert_eq!(offset.entry.x, centred.entry.x);
    }

    #[test]
    fn the_layout_follows_the_screen_it_is_given() {
        let small = plymouth(
            1280,
            720,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n"),
            Screen::Unlock,
        );
        let large = plymouth(
            3840,
            2160,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n"),
            Screen::Unlock,
        );
        assert_eq!(small.logo.centre_x(), 640);
        assert_eq!(large.logo.centre_x(), 1920);
        assert!(large.logo.y > small.logo.y);
    }

    #[test]
    fn the_greeter_draws_the_logo_at_the_width_it_is_given() {
        // The width in `Sizes` is already the theme's share of the screen;
        // the layout places it and does not size it again.
        let mut sizes = sizes();
        sizes.logo = (768, 192);
        let layout = login(1920, 1080, &sizes, &manifest("[meta]\nname = \"t\"\n"));
        assert_eq!(layout.logo.width, 768);
        assert_eq!(layout.logo.centre_x(), 960);
    }

    #[test]
    fn a_left_aligned_greeter_keeps_its_margin() {
        let layout = login(
            1920,
            1080,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n[login]\nlayout = \"left\"\n"),
        );
        assert_eq!(layout.logo.x.min(layout.lock.x), 1920 * 12 / 100);
    }

    #[test]
    fn a_right_aligned_greeter_mirrors_the_left_one() {
        let left = login(
            1920,
            1080,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n[login]\nlayout = \"left\"\n"),
        );
        let right = login(
            1920,
            1080,
            &sizes(),
            &manifest("[meta]\nname = \"t\"\n[login]\nlayout = \"right\"\n"),
        );
        let left_gap = left.lock.x.min(left.logo.x);
        let right_gap = 1920 - right.entry.x.max(right.logo.x + right.logo.width);
        assert_eq!(left_gap, 1920 * 12 / 100);
        assert!(right_gap > 0 && right.logo.x > left.logo.x);
    }

    #[test]
    fn the_greeter_column_is_vertically_centred() {
        let layout = login(1920, 1080, &sizes(), &manifest("[meta]\nname = \"t\"\n"));
        let top = layout.logo.y;
        let bottom = layout.entry.bottom().max(layout.lock.bottom());
        assert!(
            ((top + bottom) / 2 - 540).abs() <= 1,
            "column centre {} is not the screen centre",
            (top + bottom) / 2
        );
    }

    #[test]
    fn a_shutdown_override_moves_only_the_shutdown_screen() {
        let body = "[meta]\nname = \"t\"\n[logo]\nposition = \"custom\"\noffset = [10, -40]\n[logo.shutdown]\nposition = \"top\"\n";
        let unlock = plymouth(1920, 1080, &sizes(), &manifest(body), Screen::Unlock);
        let shutdown = plymouth(1920, 1080, &sizes(), &manifest(body), Screen::Shutdown);
        assert_eq!(unlock.logo.x, 1920 / 2 - 150 + 10);
        assert_eq!(
            shutdown.logo.y, 135,
            "top, and the inherited offset is ignored"
        );
        assert_eq!(shutdown.logo.x, 1920 / 2 - 150);
    }

    #[test]
    fn a_zero_height_lock_image_does_not_divide_by_zero() {
        let mut sizes = sizes();
        sizes.lock = (0, 0);
        let layout = plymouth(
            1920,
            1080,
            &sizes,
            &manifest("[meta]\nname = \"t\"\n"),
            Screen::Unlock,
        );
        assert_eq!(layout.lock.width, 0);
    }
}
