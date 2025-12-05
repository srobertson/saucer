# Effect Managers

Reference documentation for implementing effect managers in Saucer.

---

## 1. Overview

Effect managers execute side effects (HTTP, timers, file I/O) while keeping apps pure. They're discovered via Cargo.toml metadata and integrated automatically by the build system.

**Key responsibilities:**
- Execute side effects (HTTP, WebSocket, timers, file I/O)
- Manage asynchronous work (spawn tokio tasks)
- Route completion results back to application via callbacks
- Define state structure (connections, pending requests, caches)

**When to implement:**
- New platform integration (database, message queue, external API)
- New I/O capability (file system, network protocols)
- Custom async patterns (polling, streaming, workers)
- Specialized state management (caching, pooling, rate limiting)

### Key Concepts

**No traits** - Effect managers are just structs with an `on_effects` method. Discovery happens via Cargo.toml metadata, not trait implementations.

**Callbacks, not events** - Requests carry `Arc<dyn Fn(Result) -> Msg>` callbacks that produce app messages directly. No separate Event types or EventConstructors.

**Stateless handlers** - Managers define state types but don't own state. The runtime passes state in/out of `on_effects`.

**Build-time integration** - When TEA applications import crates as dependencies, the build system checks if those crates provide effect managers (via `[package.metadata.saucer]`). For discovered managers, it generates command helpers in the shared runtime and makes them available to the application.

---

## 2. Required Structure

### Cargo.toml Metadata

**REQUIRED**: Effect managers must declare themselves in `Cargo.toml`:

```toml
[package]
name = "saucer-time-manager"
# ... other package fields

[package.metadata.saucer]
effect_manager = true
request_type = "TimeRequest"      # Name of your Request enum
manager_type = "TimeManager"      # Name of your manager struct
self_msg_type = "()"              # Self-message type, or "()" if none

[dependencies]
saucer-core = { path = "../saucer-core" }
```

**Fields:**
- `effect_manager = true` - Marks this crate as discoverable
- `request_type` - Name of your Request enum (must be generic over `<Msg>`)
- `manager_type` - Name of your manager struct (usually `SomethingManager`)
- `self_msg_type` - Type for background task communication, or `"()"` if not needed

### Exclude Template Files

If your manager includes `.tea.rs` template files, exclude them from the published crate:

```toml
[package]
exclude = ["src/*.tea.rs", "**/*.tea.rs"]
```

### Module Structure

```
saucer-my-manager/
├── Cargo.toml                 # With [package.metadata.saucer]
└── src/
    ├── lib.rs                 # Request enum, Manager struct, on_effects()
    ├── requests.rs            # REQUIRED: Request helper functions
    └── command.rs             # OPTIONAL: Breadcrumb file (must be empty)
```

**Important file rules:**

1. **`requests.rs` is REQUIRED** - Request helpers MUST live here (enforced by build system)
2. **`command.rs` must be empty** - If it exists, it can only contain breadcrumb comments
3. **Request enum** can be in `lib.rs` or a separate module

---

## 3. Request Type

### Request Enum with Callbacks

Define a Request enum with `returns` callbacks:

```rust
use std::sync::Arc;

#[derive(Clone)]
pub enum TimeRequest<Msg> {
    Now {
        returns: Arc<dyn Fn(f64) -> Msg + Send + Sync>,
    },
    NotifyAfter {
        id: u64,
        duration: Duration,
        returns: Arc<dyn Fn() -> Msg + Send + Sync>,
    },
    Clear {
        id: u64,
        returns: Arc<dyn Fn() -> Msg + Send + Sync>,
    },
}
```

**Pattern:**
- Generic over `<Msg>` (the app's message type)
- Each variant stores parameters needed for the effect
- Each variant has a `returns` callback: `Arc<dyn Fn(Result) -> Msg + Send + Sync>`
- The callback signature matches what the effect produces

**Callback signatures:**
- Fire-and-forget: `Fn() -> Msg`
- With result: `Fn(T) -> Msg` where T is the effect's output
- Fallible: `Fn(Result<T, E>) -> Msg` for operations that can fail

### Implementing `.map()`

**REQUIRED**: Implement `.map()` so commands can transform message types:

```rust
impl<Msg: 'static> TimeRequest<Msg> {
    pub fn map<Msg2>(
        self,
        f: impl Fn(Msg) -> Msg2 + Send + Sync + Clone + 'static
    ) -> TimeRequest<Msg2> {
        let f = Arc::new(f);
        match self {
            TimeRequest::Now { returns } => TimeRequest::Now {
                returns: Arc::new(move |ts| f(returns(ts))),
            },
            TimeRequest::NotifyAfter { id, duration, returns } => TimeRequest::NotifyAfter {
                id,
                duration,
                returns: Arc::new(move || f(returns())),
            },
            TimeRequest::Clear { id, returns } => TimeRequest::Clear {
                id,
                returns: Arc::new(move || f(returns())),
            },
        }
    }
}
```

**Pattern:**
- Wrap mapping function in `Arc` for cloning
- For each variant, compose `f(returns(...))` in a new callback
- Preserve all other fields unchanged

### Optional: Derive Request

Use the `#[derive(Request)]` macro for automatic `.map()` generation:

```rust
use saucer_core_macros::Request;

#[derive(Clone, Request)]
pub struct HttpRequest<Msg> {
    pub url: String,
    pub method: HttpMethod,
    pub returns: Arc<dyn Fn(Result<HttpResponse, HttpError>) -> Msg + Send + Sync>,
}
```

The macro generates `.map()` for struct-style requests automatically.

---

## 4. Request Helpers

### Helper Functions in `requests.rs`

**REQUIRED**: Request helper functions MUST be defined in `src/requests.rs` (build system enforces this):

```rust
// In src/requests.rs
use super::TimeRequest;
use std::sync::Arc;
use std::time::Duration;

pub fn now<Msg: 'static>(
    returns: impl Fn(f64) -> Msg + Send + Sync + 'static
) -> TimeRequest<Msg> {
    TimeRequest::Now {
        returns: Arc::new(returns),
    }
}

pub fn notify_after<Msg: 'static>(
    id: u64,
    duration: Duration,
    returns: impl Fn() -> Msg + Send + Sync + 'static,
) -> TimeRequest<Msg> {
    TimeRequest::NotifyAfter {
        id,
        duration,
        returns: Arc::new(returns),
    }
}

pub fn clear<Msg: 'static>(
    id: u64,
    returns: impl Fn() -> Msg + Send + Sync + 'static,
) -> TimeRequest<Msg> {
    TimeRequest::Clear {
        id,
        returns: Arc::new(returns),
    }
}
```

**Best practices:**
- Name helpers after the action (imperative: `get`, `notify_after`, `subscribe`)
- Use `impl Into<T>` for string/collection parameters
- Use `impl Fn(...)` for callbacks (more flexible than `Arc<dyn Fn>`)
- Helper wraps the callback with `Arc::new(returns)`
- Keep helpers simple - just construct the request

### Why `requests.rs`?

The build system discovers effect managers by scanning the application's dependencies:

1. Reads application's `Cargo.toml` dependencies
2. Checks each dependency for `[package.metadata.saucer]` with `effect_manager = true`
3. For discovered managers, scans `<manager>/src/requests.rs` to find helper functions
4. When templates use fictional imports like `use my_manager::command::helper`:
   - Finds `helper` in `my_manager/src/requests.rs`
   - Generates a wrapped version in the shared runtime: `Cmd::single(Request::MyManager(helper(...)))`
   - Makes it available to application templates

**This is enforced** - helpers in other files won't be discovered. The `requests.rs` location is required for the build system to find them.

### Optional: `command.rs` Breadcrumb

If `src/command.rs` exists, it **MUST be empty except for comments**:

```rust
// commands are generated; see requests.rs for real helpers and request definitions.
```

The build system checks this and panics if `command.rs` contains real code.

**Purpose**: Acts as a breadcrumb pointing developers to `requests.rs` where helpers actually live.

---

## 5. Manager Implementation

### Manager Struct

Simple zero-field struct (pure interpreter):

```rust
#[derive(Copy, Clone)]
pub struct TimeManager;

impl TimeManager {
    pub fn on_effects<Msg: 'static>(
        &self,
        router: &Router<Msg, ()>,
        state: (),
        effects: Vec<TimeRequest<Msg>>,
    ) -> () {
        for request in effects {
            match request {
                TimeRequest::Now { returns } => {
                    let now = chrono::Utc::now().timestamp() as f64;
                    let msg = returns(now);
                    router.send_to_app(msg);
                }
                TimeRequest::NotifyAfter { id, duration, returns } => {
                    let router = router.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(duration).await;
                        let msg = returns();
                        router.send_to_app(msg);
                    });
                }
                TimeRequest::Clear { id, returns } => {
                    // Cancel timer logic...
                    let msg = returns();
                    router.send_to_app(msg);
                }
            }
        }
    }
}
```

**Key points:**
- Manager is a zero-field struct (stateless interpreter)
- `on_effects` method signature:
  - `&self` - immutable reference
  - `router: &Router<Msg, SelfMsg>` - for sending messages to app
  - `state: State` - owned state, mutate and return
  - `effects: Vec<YourRequest<Msg>>` - batch of requests to execute
  - Returns `State` - new/updated state
- Extract `returns` callback from each request
- Execute the effect (sync or async)
- Invoke `returns(result)` to get the `Msg`
- Send to app: `router.send_to_app(msg)`

### With State

For managers that need state:

```rust
use std::sync::Arc;
use std::collections::HashMap;

#[derive(Copy, Clone)]
pub struct HttpManager;

pub struct HttpState {
    client: Arc<HttpClient>,
    pending: HashMap<RequestId, RequestHandle>,
}

impl HttpManager {
    pub fn on_effects<Msg: 'static>(
        &self,
        router: &Router<Msg, ()>,
        mut state: HttpState,
        effects: Vec<HttpRequest<Msg>>,
    ) -> HttpState {
        for request in effects {
            let id = RequestId::new();
            let handle = state.client.request(&request.url);
            state.pending.insert(id, handle);

            let router = router.clone();
            let returns = request.returns;

            tokio::spawn(async move {
                let result = handle.await;
                let msg = returns(result);
                router.send_to_app(msg);
            });
        }
        state  // Return updated state
    }
}
```

**State rules:**
- Mutate state synchronously before spawning tasks
- State is owned - you can modify and return it
- Don't try to mutate state from async tasks (won't compile)

### With Self-Messages

For background work that needs to communicate back to the manager:

```rust
#[derive(Clone, Debug)]
pub enum HttpSelfMsg {
    RequestComplete { id: RequestId, result: Result<Response, Error> },
}

impl HttpManager {
    pub fn on_effects<Msg: 'static>(
        &self,
        router: &Router<Msg, HttpSelfMsg>,
        mut state: HttpState,
        effects: Vec<HttpRequest<Msg>>,
    ) -> HttpState {
        for request in effects {
            let id = RequestId::new();
            state.pending.insert(id.clone(), request.returns);

            let router = router.clone();
            tokio::spawn(async move {
                let result = fetch(&request.url).await;
                router.send_to_self(HttpSelfMsg::RequestComplete { id, result });
            });
        }
        state
    }

    pub fn on_self_msg<Msg: 'static>(
        &self,
        router: &Router<Msg, HttpSelfMsg>,
        mut state: HttpState,
        msg: HttpSelfMsg,
    ) -> HttpState {
        match msg {
            HttpSelfMsg::RequestComplete { id, result } => {
                if let Some(returns) = state.pending.remove(&id) {
                    let msg = returns(result);
                    router.send_to_app(msg);
                }
            }
        }
        state
    }
}
```

**When to use self-messages:**
- Tracking pending operations (store callback, retrieve on completion)
- Rate limiting, retries, timeouts
- Complex state machines

**Remember to set `self_msg_type` in Cargo.toml**:
```toml
[package.metadata.saucer]
self_msg_type = "HttpSelfMsg"
```

---

## 6. Router API

The `Router` provides communication channels:

```rust
pub struct Router<Msg, SelfMsg> {
    // Internal channels
}

impl<Msg, SelfMsg> Router<Msg, SelfMsg> {
    /// Send message to application's update()
    pub fn send_to_app(&self, msg: Msg);

    /// Send self-message to manager's on_self_msg()
    pub fn send_to_self(&self, msg: SelfMsg);
}
```

**Usage:**
- Clone router before moving into async tasks: `let router = router.clone();`
- `send_to_app(msg)` - Routes message to the app's update function
- `send_to_self(msg)` - Routes to manager's `on_self_msg` method

---

## 7. Common Patterns

### Fire-and-Forget

```rust
pub fn on_effects<Msg: 'static>(
    &self,
    router: &Router<Msg, ()>,
    state: (),
    effects: Vec<MyRequest<Msg>>,
) -> () {
    for request in effects {
        let router = router.clone();
        let returns = request.returns;

        tokio::spawn(async move {
            let result = work().await;
            let msg = returns(result);
            router.send_to_app(msg);
        });
    }
}
```

**Use when**: No need to track pending work or cancel operations.

### Track Pending Operations

```rust
pub fn on_effects<Msg: 'static>(
    &self,
    router: &Router<Msg, SelfMsg>,
    mut state: State,
    effects: Vec<MyRequest<Msg>>,
) -> State {
    for request in effects {
        let id = generate_id();
        state.pending.insert(id.clone(), request.returns);

        let router = router.clone();
        tokio::spawn(async move {
            let result = work().await;
            router.send_to_self(SelfMsg::Complete { id, result });
        });
    }
    state
}

pub fn on_self_msg<Msg: 'static>(
    &self,
    router: &Router<Msg, SelfMsg>,
    mut state: State,
    msg: SelfMsg,
) -> State {
    match msg {
        SelfMsg::Complete { id, result } => {
            if let Some(returns) = state.pending.remove(&id) {
                let msg = returns(result);
                router.send_to_app(msg);
            }
        }
    }
    state
}
```

**Use when**: Need to cancel, track, or retry operations.

### Connection Pooling

```rust
pub struct State {
    client: Arc<HttpClient>,  // Shared across requests
}

pub fn on_effects<Msg: 'static>(
    &self,
    router: &Router<Msg, ()>,
    state: State,
    effects: Vec<MyRequest<Msg>>,
) -> State {
    for request in effects {
        let client = state.client.clone();  // Clone Arc, not client
        let router = router.clone();
        let returns = request.returns;

        tokio::spawn(async move {
            let result = client.request(...).await;
            let msg = returns(result);
            router.send_to_app(msg);
        });
    }
    state
}
```

**Use when**: Expensive resources should be shared (DB connections, HTTP clients).

---

## 8. Anti-Patterns

### ❌ State in Manager Struct

```rust
// WRONG - Manager should have no fields
pub struct Manager {
    state: HashMap<K, V>,
}

// RIGHT - State separate, passed to on_effects
pub struct Manager;

pub struct State {
    data: HashMap<K, V>,
}
```

### ❌ Blocking in on_effects

```rust
// WRONG - Blocks the event loop!
pub fn on_effects<Msg: 'static>(...) -> State {
    let result = blocking_http_call();  // NO!
    state
}

// RIGHT - Spawn async work
pub fn on_effects<Msg: 'static>(...) -> State {
    tokio::spawn(async move {
        let result = async_call().await;
        router.send_to_app(msg);
    });
    state
}
```

### ❌ Mutating State in Async Tasks

```rust
// WRONG - Won't compile!
pub fn on_effects<Msg: 'static>(...) -> State {
    tokio::spawn(async move {
        state.counter += 1;  // Error: state moved
    });
    state
}

// RIGHT - Mutate synchronously, use self-messages for async updates
pub fn on_effects<Msg: 'static>(...) -> State {
    state.pending += 1;  // Mutate before async

    tokio::spawn(async move {
        let result = work().await;
        router.send_to_self(SelfMsg::Complete { result });
    });

    state  // Return immediately
}
```

### ❌ Helpers in Wrong Location

```rust
// WRONG - Build system won't find these
// In src/lib.rs
pub fn my_helper<Msg>(...) -> MyRequest<Msg> { ... }

// RIGHT - Must be in requests.rs
// In src/requests.rs
pub fn my_helper<Msg>(...) -> MyRequest<Msg> { ... }
```

---

## 9. Debugging and Logging

### ❌ DO NOT Use Tracing in Effect Managers

**Libraries should be silent.** Effect managers are library code and should NOT emit logs directly.

```rust
// ❌ WRONG
pub fn on_effects<Msg: 'static>(...) -> State {
    tracing::info!("Processing effects");  // NO!
    for request in effects {
        tracing::debug!("Handling {}", request.url);  // NO!
    }
    state
}
```

**Why not?**
- Libraries shouldn't make logging decisions for applications
- Hard-coded log levels may not match app needs
- Creates noise applications can't control

### ✅ Use Observer System Instead

Applications control observability:

```rust
// ✅ RIGHT - Silent manager
pub fn on_effects<Msg: 'static>(...) -> State {
    // Just process effects - no logging!
    for request in effects {
        execute(request);
    }
    state
}
```

**Application side:**
```rust
Runtime::new(
    app::init,
    app::update,
    app::view,
    saucer_core::no_op_reconciler(),
    saucer_core::tracing_observer(),  // App controls observability
)
```

See [Observability](./observability.md) for details.

### Error Handling

**DO** report errors through callbacks:

```rust
let msg = match risky_operation().await {
    Ok(data) => returns(Ok(data)),
    Err(e) => returns(Err(e)),  // ✅ Error as data
};
router.send_to_app(msg);
```

**DON'T** log errors in the manager:
```rust
// ❌ WRONG
Err(e) => {
    tracing::error!("Failed: {}", e);  // NO!
    returns(Err(e))
}
```

The app's `update()` decides how to handle errors (log, retry, show UI).

---

## 10. Complete Example

```rust
//! saucer-time-manager/src/lib.rs

use std::sync::Arc;
use std::time::Duration;
use saucer_core::Router;

// Request enum with callbacks
#[derive(Clone)]
pub enum TimeRequest<Msg> {
    Now {
        returns: Arc<dyn Fn(f64) -> Msg + Send + Sync>,
    },
    NotifyAfter {
        id: u64,
        duration: Duration,
        returns: Arc<dyn Fn() -> Msg + Send + Sync>,
    },
}

// Implement .map() for message transformation
impl<Msg: 'static> TimeRequest<Msg> {
    pub fn map<Msg2>(
        self,
        f: impl Fn(Msg) -> Msg2 + Send + Sync + Clone + 'static
    ) -> TimeRequest<Msg2> {
        let f = Arc::new(f);
        match self {
            TimeRequest::Now { returns } => TimeRequest::Now {
                returns: Arc::new(move |ts| f(returns(ts))),
            },
            TimeRequest::NotifyAfter { id, duration, returns } => TimeRequest::NotifyAfter {
                id,
                duration,
                returns: Arc::new(move || f(returns())),
            },
        }
    }
}

// Manager struct (zero fields)
#[derive(Copy, Clone)]
pub struct TimeManager;

impl TimeManager {
    pub fn on_effects<Msg: 'static>(
        &self,
        router: &Router<Msg, ()>,
        state: (),
        effects: Vec<TimeRequest<Msg>>,
    ) -> () {
        for request in effects {
            match request {
                TimeRequest::Now { returns } => {
                    let now = chrono::Utc::now().timestamp() as f64;
                    let msg = returns(now);
                    router.send_to_app(msg);
                }
                TimeRequest::NotifyAfter { duration, returns, .. } => {
                    let router = router.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(duration).await;
                        let msg = returns();
                        router.send_to_app(msg);
                    });
                }
            }
        }
    }
}

// Export requests module
pub mod requests;
pub use requests::{now, notify_after};
```

```rust
//! saucer-time-manager/src/requests.rs

use super::TimeRequest;
use std::sync::Arc;
use std::time::Duration;

pub fn now<Msg: 'static>(
    returns: impl Fn(f64) -> Msg + Send + Sync + 'static
) -> TimeRequest<Msg> {
    TimeRequest::Now {
        returns: Arc::new(returns),
    }
}

pub fn notify_after<Msg: 'static>(
    id: u64,
    duration: Duration,
    returns: impl Fn() -> Msg + Send + Sync + 'static,
) -> TimeRequest<Msg> {
    TimeRequest::NotifyAfter {
        id,
        duration,
        returns: Arc::new(returns),
    }
}
```

```toml
# saucer-time-manager/Cargo.toml

[package]
name = "saucer-time-manager"
version = "0.1.0"
edition = "2021"

[package.metadata.saucer]
effect_manager = true
request_type = "TimeRequest"
manager_type = "TimeManager"
self_msg_type = "()"

[dependencies]
saucer-core = { path = "../saucer-core" }
tokio = { version = "1", features = ["time"] }
chrono = "0.4"
```

---

## 11. References

- **[Main README](../../README.md)** - Saucer overview and quick start
- **[Commands (Cmd)](./cmd.md)** - How apps use commands to request effects
- **[Observability](./observability.md)** - How apps observe effects via observers
- **[Ports](./ports.md)** - Bidirectional communication with external code
- **Examples**: See [saucer-time-manager](../../saucer-time-manager/src/) and [saucer-http-manager](../../saucer-http-manager/src/)
