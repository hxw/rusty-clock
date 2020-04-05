// main.rs

use chrono::prelude::*;
use clap::{load_yaml, App};
use dirs;
use libc;
use std::ffi::CString;
use std::mem::{transmute, zeroed};
use std::os::raw::*;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};
use x11::{xft, xinput2, xlib};

mod configure;
mod socket;

const TITLE: &'static str = "Rusty Clock";
const DEFAULT_WIDTH: u32 = 480;
const DEFAULT_HEIGHT: u32 = 320;
const DEFAULT_MARGIN: i32 = 2;
const TIME_MARGIN: i32 = DEFAULT_MARGIN;
const TIME_Y: i32 = 110;
const DAY_MARGIN: i32 = DEFAULT_MARGIN;
const DAY_Y: i32 = 200;
const DATE_MARGIN: i32 = (DEFAULT_WIDTH / 2) as i32;
const DATE_Y: i32 = 200;
const WEATHER_MARGIN: i32 = DEFAULT_MARGIN;
const WEATHER_Y: i32 = 300;

const DEFAULT_CONFIG_DIR: &str = "rusty-clock";
const DEFAULT_CONFIG_FILE: &str = "rusty-clock.conf";

struct Theme {
    time: x11::xft::XftColor,
    day: x11::xft::XftColor,
    date: x11::xft::XftColor,
    weather: x11::xft::XftColor,
    background: x11::xft::XftColor,
}

pub struct ClockWindow {
    pub display: *mut xlib::Display,
    pub window: xlib::Window,

    pixmap: xlib::Pixmap,
    draw: *mut x11::xft::XftDraw,
    gc: xlib::GC,

    time_font: *mut x11::xft::XftFont,
    day_font: *mut x11::xft::XftFont,
    date_font: *mut x11::xft::XftFont,
    weather_font: *mut x11::xft::XftFont,

    time_point: configure::Point,
    day_point: configure::Point,
    date_point: configure::Point,
    weather_point: configure::Point,

    days: [String; 7],

    early: Theme,
    morning: Theme,
    afternoon: Theme,
    evening: Theme,
    unsync: Theme,

    width: u32,
    height: u32,

    wm_protocols: xlib::Atom,
    wm_delete_window: xlib::Atom,

    input: Arc<Mutex<socket::Input>>,
}

impl ClockWindow {
    /// Create a new window with a given title and size
    pub fn new(
        title: &str,
        width: u32,
        height: u32,
        days: [String; 7],
        fonts: configure::StrMap,
        coordinates: configure::PointMap,
        themes: configure::ThemeMap,
        input: Arc<Mutex<socket::Input>>,
    ) -> ClockWindow {
        unsafe {
            // Open display
            let display = xlib::XOpenDisplay(null());
            if display == null_mut() {
                panic!("can't open display");
            }

            // Load atoms
            let wm_delete_window_str = CString::new("WM_DELETE_WINDOW").unwrap();
            let wm_protocols_str = CString::new("WM_PROTOCOLS").unwrap();

            let wm_delete_window =
                xlib::XInternAtom(display, wm_delete_window_str.as_ptr(), xlib::False);

            let wm_protocols = xlib::XInternAtom(display, wm_protocols_str.as_ptr(), xlib::False);

            if wm_delete_window == 0 || wm_protocols == 0 {
                panic!("can't load atoms");
            }

            // Create window
            let screen_num = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen_num);
            let background_pixel = xlib::XBlackPixel(display, screen_num);
            let foreground_pixel = xlib::XWhitePixel(display, screen_num);

            let mut attributes: xlib::XSetWindowAttributes = zeroed();
            attributes.background_pixel = background_pixel;

            let window = xlib::XCreateWindow(
                display,
                root,
                0,
                0,
                width as c_uint,
                height as c_uint,
                0,
                0,
                xlib::InputOutput as c_uint,
                null_mut(),
                xlib::CWBackPixel,
                &mut attributes,
            );

            // Set window title
            let title_str = CString::new(title).unwrap();
            xlib::XStoreName(display, window, title_str.as_ptr() as *mut _);

            // Subscribe to delete (close) events
            let mut protocols = [wm_delete_window];

            if xlib::XSetWMProtocols(display, window, &mut protocols[0] as *mut xlib::Atom, 1)
                == xlib::False
            {
                panic!("can't set WM protocols");
            }

            let visual = xlib::XDefaultVisual(display, screen_num);
            let colourmap = xlib::XCreateColormap(display, window, visual, xlib::AllocNone);
            //let draw = xft::XftDrawCreate(display, window, visual, colourmap);

            let depth = xlib::XDefaultDepthOfScreen(xlib::XDefaultScreenOfDisplay(display));
            let pixmap = xlib::XCreatePixmap(display, window, width, height, depth as u32);
            //let draw = xft::XftDrawCreateAlpha(display, pixmap, depth);
            let draw = xft::XftDrawCreate(display, pixmap, visual, colourmap);

            let mut gcvalues: xlib::XGCValues = zeroed();
            let gc = xlib::XCreateGC(display, window, 0, &mut gcvalues);
            //xlib:: XSetFunction(self.display, gc, int function);

            xlib::XSetPlaneMask(display, gc, xlib::XAllPlanes());
            xlib::XSetForeground(display, gc, foreground_pixel);
            xlib::XSetBackground(display, gc, background_pixel);

            let time_font = ClockWindow::make_font(
                display,
                screen_num,
                fonts
                    .get("time")
                    .unwrap_or(&"Noto Sans:style=bold:size=89".to_string()),
            );
            let day_font = ClockWindow::make_font(
                display,
                screen_num,
                fonts
                    .get("day")
                    .unwrap_or(&"Noto Sans:style=bold:size=60".to_string()),
            );
            let date_font = ClockWindow::make_font(
                display,
                screen_num,
                fonts
                    .get("date")
                    .unwrap_or(&"Noto Sans:style=bold:size=60".to_string()),
            );
            let weather_font = ClockWindow::make_font(
                display,
                screen_num,
                fonts
                    .get("weather")
                    .unwrap_or(&"Noto Sans:style=bold:size=50".to_string()),
            );

            ClockWindow {
                display: display,
                window: window,
                pixmap: pixmap,
                draw: draw,
                gc: gc,
                time_font: time_font,
                day_font: day_font,
                date_font: date_font,
                weather_font: weather_font,
                time_point: *coordinates.get("time").unwrap_or(&configure::Point {
                    x: TIME_MARGIN,
                    y: TIME_Y,
                }),
                day_point: *coordinates.get("day").unwrap_or(&configure::Point {
                    x: DAY_MARGIN,
                    y: DAY_Y,
                }),
                date_point: *coordinates.get("date").unwrap_or(&configure::Point {
                    x: DATE_MARGIN,
                    y: DATE_Y,
                }),
                weather_point: *coordinates.get("weather").unwrap_or(&configure::Point {
                    x: WEATHER_MARGIN,
                    y: WEATHER_Y,
                }),

                days: days,

                early: ClockWindow::make_theme(
                    display,
                    visual,
                    colourmap,
                    themes.get("early"),
                    "SteelBlue",
                    "grey5",
                ),
                morning: ClockWindow::make_theme(
                    display,
                    visual,
                    colourmap,
                    themes.get("morning"),
                    "gold",
                    "black",
                ),
                afternoon: ClockWindow::make_theme(
                    display,
                    visual,
                    colourmap,
                    themes.get("afternoon"),
                    "pink",
                    "black",
                ),
                evening: ClockWindow::make_theme(
                    display,
                    visual,
                    colourmap,
                    themes.get("evening"),
                    "SpringGreen",
                    "grey4",
                ),
                unsync: ClockWindow::make_theme(
                    display,
                    visual,
                    colourmap,
                    themes.get("unsync"),
                    "black",
                    "red",
                ),
                width: width,
                height: height,
                wm_protocols: wm_protocols,
                wm_delete_window: wm_delete_window,
                input: input,
            }
        }
    }

    fn make_font(
        display: *mut x11::xlib::Display,
        screen_num: c_int,
        name: &str,
    ) -> *mut x11::xft::XftFont {
        let font_str = CString::new(name).unwrap();
        unsafe { xft::XftFontOpenName(display, screen_num, font_str.as_ptr() as *mut _) }
    }

    fn make_theme(
        display: *mut x11::xlib::Display,
        visual: *const x11::xlib::Visual,
        colourmap: x11::xlib::Colormap,
        theme: Option<&configure::StrMap>,
        foreground: &str,
        background: &str,
    ) -> Theme {
        let mut time_colour = foreground;
        let mut day_colour = foreground;
        let mut date_colour = foreground;
        let mut weather_colour = foreground;
        let mut background_colour = background;
        match theme {
            Some(t) => {
                match t.get("time") {
                    Some(c) => time_colour = c,
                    None => (),
                };
                match t.get("day") {
                    Some(c) => day_colour = c,
                    None => (),
                };
                match t.get("date") {
                    Some(c) => date_colour = c,
                    None => (),
                };
                match t.get("weather") {
                    Some(c) => weather_colour = c,
                    None => (),
                };
                match t.get("background") {
                    Some(c) => background_colour = c,
                    None => (),
                };
            }
            None => (),
        };
        Theme {
            time: ClockWindow::make_colour(display, visual, colourmap, time_colour),
            day: ClockWindow::make_colour(display, visual, colourmap, day_colour),
            date: ClockWindow::make_colour(display, visual, colourmap, date_colour),
            weather: ClockWindow::make_colour(display, visual, colourmap, weather_colour),
            background: ClockWindow::make_colour(display, visual, colourmap, background_colour),
        }
    }

    fn make_colour(
        display: *mut x11::xlib::Display,
        visual: *const x11::xlib::Visual,
        colourmap: x11::xlib::Colormap,
        name: &str,
    ) -> x11::xft::XftColor {
        unsafe {
            let mut colour: xft::XftColor = zeroed();
            let c_str = CString::new(name).unwrap();
            xft::XftColorAllocName(
                display,
                visual,
                colourmap,
                c_str.as_ptr() as *mut _,
                &mut colour,
            );
            colour
        }
    }

    fn fullscreen(&mut self) {
        let net_wm_state_fullscreen_str = CString::new("_NET_WM_STATE_FULLSCREEN").unwrap();
        let net_wm_state_str = CString::new("_NET_WM_STATE").unwrap();
        unsafe {
            let atoms = [
                xlib::XInternAtom(
                    self.display,
                    net_wm_state_fullscreen_str.as_ptr(),
                    xlib::False,
                ),
                0,
            ];
            xlib::XChangeProperty(
                self.display,
                self.window,
                xlib::XInternAtom(self.display, net_wm_state_str.as_ptr(), xlib::False),
                xlib::XA_ATOM,
                32,
                xlib::PropModeReplace,
                atoms.as_ptr() as *const u8,
                1,
            );
        }
    }

    /// Display the window
    pub fn show(&mut self) {
        unsafe {
            xlib::XMapWindow(self.display, self.window);

            let dt = Local::now();

            let t = dt.format("%H:%M:%S").to_string();
            let time_len = t.len() as i32;
            let time_str = CString::new(t).unwrap();

            let d = &self.days[dt.weekday().num_days_from_sunday() as usize];
            let day_len = d.len() as i32;
            let day_str = CString::new(d.to_string()).unwrap();

            // let d = dt.format("%Y-%m-%d").to_string();
            let d = dt.format("%m-%d").to_string();
            let date_len = d.len() as i32;
            let date_str = CString::new(d).unwrap();

            let (theme, weather, temperature) = {
                let f = self.input.lock().unwrap();
                let theme = if (*f).sync {
                    match dt.hour() {
                        0 | 1 | 2 | 3 | 4 | 5 => &self.early,
                        6 | 7 | 8 | 9 | 10 | 11 => &self.morning,
                        12 | 13 | 14 | 15 | 16 | 17 => &self.afternoon,
                        _ => &self.evening,
                    }
                } else {
                    &self.unsync
                };
                (theme, (*f).weather.clone(), (*f).temperature.clone())
            };
            //let weather_len = weather.len() as i32;

            xft::XftDrawRect(self.draw, &theme.background, 0, 0, self.width, self.height);

            xft::XftDrawStringUtf8(
                self.draw,
                &theme.time,
                self.time_font,
                self.time_point.x,
                self.time_point.y,
                time_str.as_ptr() as *mut _,
                time_len,
            );
            xft::XftDrawStringUtf8(
                self.draw,
                &theme.day,
                self.day_font,
                self.day_point.x,
                self.day_point.y,
                day_str.as_ptr() as *mut _,
                day_len,
            );
            xft::XftDrawStringUtf8(
                self.draw,
                &theme.date,
                self.date_font,
                self.date_point.x,
                self.date_point.y,
                date_str.as_ptr() as *mut _,
                date_len,
            );

            let w_count = weather.char_indices().count();
            let t_count = weather.char_indices().count();

            let mut ww = String::new();

            // need some way to move the length to config
            if w_count + 1 + t_count < 8 {
                ww.push_str(&weather);
                ww.push_str(" ");
                ww.push_str(&temperature);
            } else if dt.second() / 2 % 2 == 0 {
                ww.push_str(&weather);
            } else {
                ww.push_str(&temperature);
            };
            xft::XftDrawStringUtf8(
                self.draw,
                &theme.weather,
                self.weather_font,
                self.weather_point.x,
                self.weather_point.y,
                ww.as_ptr() as *mut _,
                ww.len() as i32,
            );

            xlib::XCopyArea(
                self.display,
                self.pixmap,
                self.window,
                self.gc,
                0,
                0,
                self.width,
                self.height,
                0,
                0,
            );
        }
    }

    /// Process events for the window. Window close events are handled automatically,
    /// other events are passed on to |event_handler|
    pub fn run_event_loop<EventHandler>(&mut self, mut event_handler: EventHandler)
    where
        EventHandler: FnMut(&xlib::XEvent) -> bool,
    {
        let x11_fd = unsafe { xlib::XConnectionNumber(self.display) };

        let mut event: xlib::XEvent = unsafe { zeroed() };
        let mut in_fds: libc::fd_set = unsafe { zeroed() };

        'event_loop: loop {
            let mut tv = libc::timeval {
                tv_usec: 0, // 500_000,
                tv_sec: 1,  // 0,
            };

            // Create a File Description Set containing x11_fd
            unsafe {
                libc::FD_ZERO(&mut in_fds);
                libc::FD_SET(x11_fd, &mut in_fds);
            }

            let status =
                unsafe { libc::select(x11_fd + 1, &mut in_fds, null_mut(), null_mut(), &mut tv) };
            if status == 0 {
                // println!("timer tick");
                self.show();
            }

            loop {
                let f = unsafe { xlib::XPending(self.display) };
                if f == 0 {
                    break;
                }

                unsafe { xlib::XNextEvent(self.display, &mut event) };
                match event.get_type() {
                    xlib::ClientMessage => {
                        let xclient: xlib::XClientMessageEvent = From::from(event);

                        // WM_PROTOCOLS client message
                        if xclient.message_type == self.wm_protocols && xclient.format == 32 {
                            let protocol = xclient.data.get_long(0) as xlib::Atom;

                            // WM_DELETE_WINDOW (close event)
                            if protocol == self.wm_delete_window {
                                break 'event_loop;
                            }
                        }
                    }
                    _ => {
                        if !event_handler(&event) {
                            break 'event_loop;
                        }
                    }
                }
            }
        }
    }
}

impl Drop for ClockWindow {
    /// Destroys the window and disconnects from the display
    fn drop(&mut self) {
        unsafe {
            xlib::XDestroyWindow(self.display, self.window);
            xlib::XCloseDisplay(self.display);
        }
    }
}

// entry point
fn main() {
    // The YAML file is found relative to the current file, similar to how modules are found
    let yaml = load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches();

    let debug = matches.is_present("debug");

    if debug {
        match matches.occurrences_of("verbose") {
            0 => println!("verbose mode is off"),
            1 => println!("verbose mode is low"),
            2 => println!("verbose mode is on"),
            3 | _ => println!("maximum verbosity"),
        }
    }

    let fullscreen = matches.is_present("fullscreen");

    let mut config = std::path::PathBuf::new();

    match matches.value_of("config") {
        Some(name) => config.push(name),
        None => match dirs::config_dir() {
            Some(d) => {
                config.push(d);
                config.push(DEFAULT_CONFIG_DIR);
                config.push(DEFAULT_CONFIG_FILE);
            }
            None => panic!("cannot get home directory"),
        },
    };

    // end of options processing
    // read configurartion file
    let cfg = configure::read(&config, debug).expect("error in config file");

    // setup socket
    let sync_flag = socket::setup(&cfg.socket, debug).unwrap();

    // setup window
    let width = if cfg.width > 0 {
        cfg.width as u32
    } else {
        DEFAULT_WIDTH
    };
    let height = if cfg.height > 0 {
        cfg.height as u32
    } else {
        DEFAULT_HEIGHT
    };

    let mut clock_window = ClockWindow::new(
        TITLE,
        width,
        height,
        cfg.days,
        cfg.fonts,
        cfg.coordinates,
        cfg.themes,
        sync_flag,
    );
    if fullscreen {
        clock_window.fullscreen();
    }

    // query XInput support
    let mut opcode: c_int = 0;
    let mut event: c_int = 0;
    let mut error: c_int = 0;
    let xinput_str = CString::new("XInputExtension").unwrap();
    let xinput_available = unsafe {
        xlib::XQueryExtension(
            clock_window.display,
            xinput_str.as_ptr(),
            &mut opcode,
            &mut event,
            &mut error,
        )
    };
    if xinput_available == xlib::False {
        panic!("XInput not available")
    }

    let mut xinput_major_ver = xinput2::XI_2_Major;
    let mut xinput_minor_ver = xinput2::XI_2_Minor;
    if unsafe {
        xinput2::XIQueryVersion(
            clock_window.display,
            &mut xinput_major_ver,
            &mut xinput_minor_ver,
        )
    } != xlib::Success as c_int
    {
        panic!("XInput2 not available");
    }
    if debug {
        println!(
            "XI version available {}.{}",
            xinput_major_ver, xinput_minor_ver
        );
    }

    // init XInput events
    let mut mask: [c_uchar; 1] = [0];
    let mut input_event_mask = xinput2::XIEventMask {
        deviceid: xinput2::XIAllMasterDevices,
        mask_len: mask.len() as i32,
        mask: mask.as_mut_ptr(),
    };
    let events = &[
        xinput2::XI_ButtonPress,
        xinput2::XI_ButtonRelease,
        xinput2::XI_KeyPress,
        xinput2::XI_KeyRelease,
        xinput2::XI_Motion,
    ];
    for &event in events {
        xinput2::XISetMask(&mut mask, event);
    }

    match unsafe {
        xinput2::XISelectEvents(
            clock_window.display,
            clock_window.window,
            &mut input_event_mask,
            1,
        )
    } {
        status if status as u8 == xlib::Success => (),
        err => panic!("Failed to select events {:?}", err),
    }

    // Show window
    clock_window.show();

    // Main loop
    let display = clock_window.display;

    // event callback can return false to exit
    clock_window.run_event_loop(|event| match event.get_type() {
        xlib::GenericEvent => {
            let mut cookie: xlib::XGenericEventCookie = From::from(*event);
            if unsafe { xlib::XGetEventData(display, &mut cookie) } != xlib::True {
                if debug {
                    println!("Failed to retrieve event data");
                }
                return true;
            }
            let mut can_continue = true;
            match cookie.evtype {
                xinput2::XI_KeyPress | xinput2::XI_KeyRelease => {
                    let event_data: &xinput2::XIDeviceEvent = unsafe { transmute(cookie.data) };
                    if cookie.evtype == xinput2::XI_KeyPress {
                        if event_data.flags & xinput2::XIKeyRepeat == 0 {
                            println!("Key {} pressed", event_data.detail);
                        }
                    } else {
                        println!("Key {} released", event_data.detail);
                    }

                    const KEY_ESCAPE: xlib::KeySym = x11::keysym::XK_Escape as xlib::KeySym;

                    let sym = unsafe {
                        xlib::XkbKeycodeToKeysym(event_data.display, event_data.detail as u8, 0, 0)
                    };

                    can_continue = sym != KEY_ESCAPE; // exit if Esc key pressed
                }
                xinput2::XI_ButtonPress | xinput2::XI_ButtonRelease => {
                    let event_data: &xinput2::XIDeviceEvent = unsafe { transmute(cookie.data) };
                    if cookie.evtype == xinput2::XI_ButtonPress {
                        println!("Button {} pressed", event_data.detail);
                    } else {
                        println!("Button {} released", event_data.detail);
                    }
                }
                xinput2::XI_Motion => {
                    if debug {
                        println!("motion event");
                    }
                }
                _ => {
                    println!("what event");
                }
            }
            unsafe { xlib::XFreeEventData(display, &mut cookie) };
            can_continue
        }
        _ => {
            if debug {
                println!("some other event");
            }
            true
        }
    });
}
