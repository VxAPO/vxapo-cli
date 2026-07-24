mod app;
mod display;
mod endpoint;
mod knowledge;
mod probe;
mod reg;
mod regdump;

use std::io::Write;

use app::App;

fn main() {
    println!("VxAPO CLI");
    println!("====================\n");

    let mut app = App::new();
    app.refresh();

    if app.endpoints.is_empty() {
        println!("No audio endpoints found.");
        return;
    }

    display::print_endpoints(&app.endpoints);

    loop {
        println!("\n--- Commands ---");
        println!("[0-{}]  Select endpoint", app.endpoints.len() - 1);
        println!("[r]    Refresh");
        println!("[q]    Quit");
        print!("> ");
        std::io::stdout().flush().unwrap();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        match input {
            "q" | "quit" | "exit" => break,
            "r" | "refresh" => {
                app.refresh();
                display::print_endpoints(&app.endpoints);
            }
            n => {
                if let Ok(idx) = n.parse::<usize>() {
                    if idx < app.endpoints.len() {
                        let ep = &app.endpoints[idx];
                        display::print_detail_header(ep);

                        loop {
                            println!("\n  [{}x{}] View registry  {}[b]{} Back", display::CYAN, display::RESET, display::GRAY, display::RESET);
                            print!("  > ");
                            std::io::stdout().flush().unwrap();

                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input).unwrap();

                            match input.trim() {
                                "x" => regdump::dump_endpoint(ep),
                                "b" | "q" | "" => break,
                                _ => {}
                            }
                        }
                        display::print_endpoints(&app.endpoints);
                    } else {
                        println!("Index out of range.");
                    }
                } else {
                    println!("Unknown command.");
                }
            }
        }
    }
}