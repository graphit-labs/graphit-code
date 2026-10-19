//! Bounded native context codecs. Encoding grants no authority and does not
//! establish that a host executed a hook or delivered its output to the model.
mod output;
mod roles;
pub use output::{
    DeliveryChannel, EncodedOutput, HookEvent, OutputContext, OutputError, OutputLimits,
    OutputMessage, encode,
};
pub use roles::{ManagedRole, RoleError, StaticRole, compose_role};

pub mod instructions;
pub mod policy_instructions;
pub mod skill_instructions;

pub mod role_instructions;
