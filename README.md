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

```rust
use saucer::{Cmd, command::shutdown};
use chrono::{DateTime, Utc};

pub struct Model {
    pub current_time: Option<String>,
}

pub enum Msg {
    GotTime(String),
    Done,
}

pub fn init() -> (Model, Cmd<Msg>) {
    let model = Model { current_time: None };
    let cmd = time_now(|ts| ts)
        .map(format_timestamp)
        .map(Msg::GotTime);
    (model, cmd)
}

pub fn update(model: Model, msg: Msg) -> (Model, Cmd<Msg>) {
    match msg {
        Msg::GotTime(formatted) => {
            let model = Model { current_time: Some(formatted), ..model };
            println!("Current time: {}", model.current_time.as_ref().unwrap());
            (model, shutdown())
        }
        Msg::Done => (model, Cmd::none()),
    }
}

pub fn view(_model: &Model) -> () {}

fn main() {
    Runtime::run(init, update, view);
}
```

Stay tuned for the release!

## License

MIT License - see [LICENSE](LICENSE) for details.
