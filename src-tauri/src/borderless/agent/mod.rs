//! # Borderless Agent
//!
//! Portable, framework-agnostic agentic AI toolkit for Rust.
//!
//! ## Quick Start
//!
//! ```no_run
//! use crate::borderless::agent::AgentBuilder;
//! use crate::borderless::agent_core::LlmConfig;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut agent = AgentBuilder::new()
//!     .set_llm_config(LlmConfig {
//!         api_key: "sk-...".into(),
//!         model: Some("gpt-4o".into()),
//!         ..Default::default()
//!     })
//!     .build()?;
//!
//! let result = agent.chat("Hello!", None).await?;
//! println!("{}", result.reply);
//! # Ok(())
//! # }
//! ```

pub mod builder;
pub mod instance;
pub mod harness;
pub mod agent_loop;
pub mod autonomous_loop;

// Re-export key types for convenience
pub use builder::AgentBuilder;
pub use instance::AgentInstance;

// Re-export sub-crates for one-stop access
pub use crate::borderless::agent_core as core;
pub use crate::borderless::telemetry as telemetry;
pub use crate::borderless::context as context;
pub use crate::borderless::providers as providers;
pub use crate::borderless::tools as tools;
pub use crate::borderless::skills as skills;
pub use crate::borderless::memory as memory;
pub use crate::borderless::session as session;
