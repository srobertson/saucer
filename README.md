# Saucer - The Elm Architecture for Rust

![Saucer Tea Party](docs/assets/tea-party-hero.png)

**Status: Coming Soon**

Saucer is a Rust framework implementing The Elm Architecture (TEA) - a pattern for building robust, maintainable applications with:

- **Pure Business Logic**: Model and update functions with no side effects
- **Explicit Side Effects**: Commands and subscriptions for controlled external interactions
- **Type Safety**: Leverage Rust's type system for compile-time guarantees
- **Composability**: Build complex applications from simple, testable components

The Elm Architecture provides a simple pattern: `Model + Event → (Model, Commands)`, making your application logic predictable, testable, and easy to reason about.

## Example

Saucer uses a build script to generate type-safe runtime code. Write your app logic in pure Rust:

**src/app.rs:**
```rust
// Your pure business logic - no side effects
use saucer_core::Cmd;
use saucer_time::command::time_now;

pub struct Model {
    current_time: Option<String>,
}

pub enum Msg {
    GotTime(f64),
}

pub fn init() -> (Model, Cmd<Msg>) {
    (
        Model { current_time: None },
        time_now(Msg::GotTime)
    )
}

pub fn update(model: Model, msg: Msg) -> (Model, Cmd<Msg>) {
    match msg {
        Msg::GotTime(timestamp) => {
            let time_str = format!("Time: {}", timestamp);
            println!("{}", time_str);
            (
                Model { current_time: Some(time_str) },
                Cmd::none()
            )
        }
    }
}
```

**src/main.rs:**
```rust
mod runtime {
    include!(concat!(env!("OUT_DIR"), "/runtime.rs"));
}

use runtime::{Runtime, app};

fn main() {
    Runtime::run(app::init, app::update);
}
```

**build.rs:**
```rust
fn main() {
    saucer_core::build::generate_runtime();
}
```

The build script discovers your effect managers and generates type-safe `Cmd` wrappers automatically.

Stay tuned for the release!

## License

MIT License - see [LICENSE](LICENSE) for details.
