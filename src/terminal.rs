use anyhow::Result;
use libc;
use std::io::Write;

static mut ORIG_TERMIOS: Option<libc::termios> = None;

pub fn enable_raw_mode() {
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) == 0 {
            ORIG_TERMIOS = Some(termios);
            let mut raw = termios;
            libc::cfmakeraw(&mut raw);
            raw.c_lflag |= libc::ISIG; // Ctrl+C pour quitter
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
        }
    }
}

pub fn disable_raw_mode() {
    unsafe {
        if let Some(ref termios) = ORIG_TERMIOS {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, termios);
        }
    }
}

extern "C" fn on_exit() {
    disable_raw_mode();
}

extern "C" fn handle_signal(sig: libc::c_int) {
    disable_raw_mode();
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

pub fn install_handlers() {
    unsafe {
        libc::atexit(on_exit);
        libc::signal(libc::SIGINT, handle_signal as libc::sighandler_t);
        libc::signal(libc::SIGTERM, handle_signal as libc::sighandler_t);
    }
}

pub fn print_banner(debug: bool) -> Result<()> {
    if debug {
        println!("\n┌──────────────────────────────────────────────────┐");
        println!("│  Console serie active - mode raw                  │");
        println!("│  Fleches / Tab / touches speciales : OK           │");
        println!("│  Ctrl+C pour arreter la VM.                       │");
        println!("│                                                    │");
        println!("│  Login: root    Password: root                    │");
        println!("│  (defini via cloud-init seed)                     │");
        println!("└──────────────────────────────────────────────────┘");
        std::io::stdout().flush()?;
    }
    Ok(())
}
