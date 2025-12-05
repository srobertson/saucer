# Saucer - The Elm Architecture for Rust

![Saucer Tea Party](docs/assets/tea-party-hero.png)

**Status: Internal preview**

Saucer is a Rust framework implementing The Elm Architecture (TEA) - a pattern for building robust, maintainable applications with:

- **Pure Business Logic**: Model and update functions with no side effects
- **Explicit Side Effects**: Commands and subscriptions for controlled external interactions
- **Type Safety**: Leverage Rust's type system for compile-time guarantees
- **Composability**: Build complex applications from simple, testable components

The Elm Architecture provides a simple pattern: `Model + Event → (Model, Commands)`, making your application logic predictable, testable, and easy to reason about.

## Example

Saucer ships a synchronous TEA runtime, a codegen pipeline (the same one used in the cmd-poc), and a ports system for host/app bridging. Write your app logic in pure Rust:

**src/app.rs:**
```rust
// Your pure business logic - no side effects
use saucer_core::Cmd;
use saucer_time_manager::command::time_now;

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

#[tokio::main]
async fn main() {
    Runtime::new(
        app::init,
        app::update,
        app::view,
        saucer_core::no_op_reconciler(),
        saucer_core::tracing_observer(),
    )
    .run()
    .await;
}
```

**build.rs:**
```rust
fn main() {
    saucer_core::build::generate_runtime();
}
```

The build script discovers your effect managers (declared via `[package.metadata.saucer]`) and generates type-safe `Cmd` wrappers automatically, keeping all template imports fictional so permissions stay enforced.

## How It Works

### The Elm Compiler Approach in Rust

Elm's famous guarantee: **"If it compiles, it works"** - no runtime exceptions. The Elm compiler achieves this by:
- Analyzing your code to discover which effect managers you use
- Generating runtime code with type-safe command helpers
- Catching configuration errors at compile-time (no runtime surprises)
- Ensuring every effect request matches your application's message type

**Saucer brings this to Rust** by using Cargo's build system as the "compiler":

1. **Discovery**: Scans your dependencies for effect managers (via `[package.metadata.saucer]`)
2. **Generation**: Creates a shared runtime with type-safe command helpers
3. **Type Safety**: Ensures commands match your `Msg` type at compile-time
4. **No Type Erasure**: Unlike other approaches, all effect types are statically known at build time
5. **Open/Closed Principle**: Open at development time (easy to add effects), closed at build time (no runtime errors)

**Goal: "If it compiles, it works"** - achieve Elm's runtime guarantees within Rust's existing tooling.

### Build-Time Code Generation

The build system acts as the compiler:

1. **Effect managers declare themselves** via Cargo.toml metadata:
   ```toml
   [package.metadata.saucer]
   effect_manager = true
   request_type = "HttpRequest"
   ```

2. **Templates use fictional imports** that get rewritten:
   ```rust
   // In your app.tea.rs:
   use http_manager::command::get;  // Fictional path

   // Gets rewritten to real path in generated code
   ```

3. **Generator produces runtime.rs** with:
   - Unified request enum
   - Type-safe Cmd wrappers
   - Runtime with event loop

### Commands and Effects

Commands are data describing side effects:

```rust
// Prefer passing variant constructors directly
http::get("https://api.example.com", Msg::GotData)

// Use closures when you need to transform
http::get("https://api.example.com", |result| {
    match result {
        Ok(data) => Msg::Success(data),
        Err(err) => Msg::Failed(err.message),
    }
})

// Compose multiple commands
cmd1.and(cmd2).and(cmd3)
```

### Fictional Imports = Compile-Time Permissions

Apps can't bypass permission checks:

```rust
// ✅ Can use helpers (fictional import rewritten)
use http_manager::command::get;
let cmd = get("https://api.example.com", Msg::Got);

// ❌ Can't construct requests directly (no public constructor)
let req = HttpRequest { url: "...", ... };  // Won't compile!
```

### What's Included

- **Core runtime primitives**: `Cmd`, `Router`, `SendToManager`, observation/observer hooks
- **Build-time generator**: Parses `.tea.rs` templates, rewrites fictional imports, generates `runtime.rs`
- **Sync runtime**: Deterministic, perfect for testing (async support planned)

## Documentation

- **[Getting Started](docs/README.md)** - Complete guide to using Saucer
- **[Architecture](docs/architecture/)** - Core concepts and design
- **[Guides](docs/guides/)** - Step-by-step tutorials

## License

MIT License - see [LICENSE](LICENSE) for details.
