use clap::{Parser, Subcommand};
use crate::AppState;

#[derive(Parser, Debug)]
#[command(name = "pale-server", about = "Pale server administration CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: CliCommand,
}

#[derive(Subcommand, Debug)]
pub enum CliCommand {
    /// User management
    User {
        #[command(subcommand)]
        action: UserAction,
    },
    /// Policy management
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },
    /// Audit log management
    Audit {
        #[command(subcommand)]
        action: AuditAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum UserAction {
    /// List all users
    List,
    /// Create a new user
    Create {
        /// Username (SIP URI prefix)
        #[arg(long)]
        username: String,
        /// Display name
        #[arg(long)]
        display_name: String,
        /// Password
        #[arg(long)]
        password: String,
        /// Role (user or admin)
        #[arg(long, default_value = "user")]
        role: String,
    },
    /// Deactivate a user
    Deactivate {
        /// User ID (UUID)
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum PolicyAction {
    /// List conditional access policies
    List,
}

#[derive(Subcommand, Debug)]
pub enum AuditAction {
    /// Export audit events as CSV
    Export {
        /// Output file path (defaults to stdout)
        #[arg(long)]
        output: Option<String>,
    },
}

pub fn run_cli(cli: Cli, state: &AppState) {
    match cli.command {
        CliCommand::User { action } => match action {
            UserAction::List => {
                let users = state.all_users();
                println!("{:<38} {:<30} {:<30} {:<8} {:<8}", "ID", "SIP URI", "Display Name", "Role", "Active");
                println!("{}", "-".repeat(114));
                for user in users {
                    println!(
                        "{:<38} {:<30} {:<30} {:<8} {:<8}",
                        user.id,
                        user.sip_uri,
                        user.display_name,
                        user.role,
                        if user.active { "yes" } else { "no" }
                    );
                }
            }
            UserAction::Create { username, display_name, password, role } => {
                let sip_uri = if username.starts_with("sip:") {
                    username.clone()
                } else {
                    format!("sip:{}", username)
                };
                let req = crate::CreateUserRequest {
                    display_name: display_name.clone(),
                    sip_uri,
                    password: Some(password),
                    role: Some(role.clone()),
                    matrix_user_id: None,
                };
                match state.create_user(req) {
                    Ok(user) => println!("Created user: {} ({})", user.display_name, user.id),
                    Err(e) => eprintln!("Failed to create user: {}", e),
                }
            }
            UserAction::Deactivate { id } => {
                match uuid::Uuid::parse_str(&id) {
                    Ok(uuid) => {
                        if state.set_user_active(uuid, false, "cli-admin").is_some() {
                            println!("User {} deactivated", id);
                        } else {
                            eprintln!("User {} not found", id);
                        }
                    }
                    Err(_) => eprintln!("Invalid UUID: {}", id),
                }
            }
        },
        CliCommand::Policy { action } => match action {
            PolicyAction::List => {
                let policies = state.list_conditional_access_policies();
                println!("{:<38} {:<30} {:<8}", "ID", "Name", "Enabled");
                println!("{}", "-".repeat(76));
                for policy in policies {
                    println!(
                        "{:<38} {:<30} {:<8}",
                        policy.id,
                        policy.name,
                        if policy.enabled { "yes" } else { "no" }
                    );
                }
            }
        },
        CliCommand::Audit { action } => match action {
            AuditAction::Export { output } => {
                let events = state.audit_events();
                let mut csv = String::from("created_at,principal,action,target\n");
                for event in &events {
                    csv.push_str(&format!(
                        "{},{},{},{}\n",
                        event.created_at,
                        event.principal,
                        event.action,
                        event.target.as_deref().unwrap_or("")
                    ));
                }
                match output {
                    Some(path) => {
                        match std::fs::write(&path, &csv) {
                            Ok(()) => println!("Exported {} audit events to {}", events.len(), path),
                            Err(e) => eprintln!("Failed to write file: {}", e),
                        }
                    }
                    None => print!("{}", csv),
                }
            }
        },
    }
}
