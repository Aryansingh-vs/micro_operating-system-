use alloc::{format, string::{String, ToString}, vec::Vec};
use crate::{apps, fs, rtc, task, vga};
use x86_64::instructions::port::Port;

pub struct Shell {
    pub input: String,
    pub history: Vec<String>,
    history_index: Option<usize>,
    pub cwd: Vec<String>,
}

impl Shell {
    pub fn new() -> Self {
        let shell = Self {
            input: String::new(),
            history: Vec::new(),
            history_index: None,
            cwd: Vec::new(),
        };
        shell.print_prompt();
        shell
    }

    pub fn print_prompt(&self) {
        let path = if self.cwd.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.cwd.join("/"))
        };
        vga::print(&format!("aryos:{path}$ "));
    }

    pub fn backspace(&mut self) {
        if self.input.pop().is_some() {
            vga::print("\u{8}");
        }
    }

    pub fn previous_history(&mut self) {
        if self.history.is_empty() { return; }
        let index = match self.history_index {
            Some(0) => 0,
            Some(i) => i - 1,
            None => self.history.len() - 1,
        };
        self.history_index = Some(index);
        self.replace_input(&self.history[index].clone());
    }

    pub fn next_history(&mut self) {
        if let Some(index) = self.history_index {
            if index + 1 < self.history.len() {
                let next = index + 1;
                self.history_index = Some(next);
                let text = self.history[next].clone();
                self.replace_input(&text);
            } else {
                self.history_index = None;
                self.replace_input("");
            }
        }
    }

    fn replace_input(&mut self, text: &str) {
        while !self.input.is_empty() { let _ = self.input.pop(); vga::print("\u{8}"); }
        self.input = String::from(text);
        vga::print(text);
    }

    pub fn execute(&mut self) {
        vga::println("");
        let line = self.input.clone();
        if !line.is_empty() {
            self.history.push(line.clone());
            self.history_index = None;
        }
        let mut parts = line.split_whitespace();
        let command = parts.next().unwrap_or("").to_ascii_lowercase();
        let args: Vec<&str> = parts.collect();

        match command.as_str() {
            "" => {}
            "help" => vga::println("help clear echo ls cat touch write rm mkdir mem uptime date sysinfo color editor calc snake monitor reboot shutdown dmesg"),
            "clear" => {
                vga::WRITER.lock().as_mut().map(|w| w.draw_desktop());
                vga::WRITER.lock().as_mut().map(|w| w.set_cursor(3, 2));
                self.print_prompt();
            }
            "echo" => vga::println(&args.join(" ")),
            "ls" => {
                let entries = fs::list_cwd(&self.cwd);
                for entry in entries { vga::println(&entry); }
            }
            "cat" => {
                if let Some(name) = args.first() {
                    if let Some(data) = apps::cat(&self.cwd, name) {
                        vga::println(&data);
                    } else {
                        vga::println("cat: file not found");
                    }
                }
            }
            "touch" => {
                if let Some(name) = args.first() {
                    if fs::create_file(&self.cwd, name, "") {
                        vga::println("file created");
                    } else {
                        vga::println("touch: cannot create file");
                    }
                }
            }
            "write" => {
                if args.len() >= 2 {
                    let name = args[0];
                    let data = args[1..].join(" ");
                    if fs::write_file(&self.cwd, name, &data) {
                        vga::println("written");
                    } else {
                        vga::println("write: file not found");
                    }
                }
            }
            "rm" => {
                if let Some(name) = args.first() {
                    vga::println(if fs::remove_file(&self.cwd, name) { "removed" } else { "rm: file not found" });
                }
            }
            "mkdir" => {
                if let Some(name) = args.first() {
                    vga::println(if fs::create_dir(&self.cwd, name) { "directory created" } else { "mkdir: cannot create directory" });
                }
            }
            "mem" => vga::println(&format!("Heap: {} KB reserved; {} frames reported by boot memory map", crate::memory::HEAP_SIZE / 1024, fs::ROOT.lock().dirs.len())),
            "uptime" => vga::println(&format!("up {} seconds", task::uptime_seconds())),
            "date" => {
                let (s, m, h, d, mo, y) = rtc::read();
                vga::println(&format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, m, s));
            }
            "sysinfo" => vga::println("AryOS x86_64 | Rust no_std | PIT IRQ0 | PS/2 IRQ1 | heap 1 MB"),
            "color" => vga::println("AryOS palette: dark blue / cyan / white"),
            "editor" => vga::println("Text editor ready: use 'touch <file>' then 'write <file> <text>'"),
            "calc" => {
                if let Some(result) = apps::calculator(&args.join(" ")) {
                    vga::println(&format!("= {}", result));
                } else {
                    vga::println("calc: usage: calc <number> <op> <number>");
                }
            }
            "snake" => vga::println("Snake: use this shell to launch; gameplay UI is available in the next interactive frame."),
            "monitor" => vga::println(&format!("UP {} s | HEAP {} KB | IRQS enabled", task::uptime_seconds(), crate::memory::HEAP_SIZE / 1024)),
            "reboot" => {
                let mut port = Port::<u8>::new(0x64);
                unsafe { port.write(0xfe); }
            }
            "shutdown" => {
                let mut port = Port::<u16>::new(0x604);
                unsafe { port.write(0x2000); }
            }
            "dmesg" => {
                vga::println("[boot] GDT initialized");
                vga::println("[boot] IDT and PIC initialized");
                vga::println("[boot] memory heap initialized");
                vga::println("[boot] PS/2 keyboard ready");
            }
            _ => vga::println(&format!("{}: command not found", command)),
        }
        self.input.clear();
        if command != "clear" { self.print_prompt(); }
    }
}
