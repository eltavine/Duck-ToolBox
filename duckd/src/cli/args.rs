use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "duckd")]
#[command(about = "Duck ToolBox backend", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Artifacts {
        #[command(subcommand)]
        command: ArtifactCommands,
    },
    Rkp {
        #[command(subcommand)]
        command: RkpCommands,
    },
}

#[derive(Subcommand)]
pub enum ArtifactCommands {
    List(JsonOnlyArgs),
}

#[derive(Subcommand)]
pub enum RkpCommands {
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },
    Info(InfoArgs),
    Provision(ProvisionArgs),
    Keybox(KeyboxArgs),
    Verify(VerifyArgs),
}

#[derive(Subcommand)]
pub enum ProfileCommands {
    Show(ProfileShowArgs),
    Save(ProfileSaveArgs),
    Clear(ProfileClearArgs),
}

#[derive(Args, Clone)]
pub struct JsonOnlyArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Clone)]
pub struct ProfileShowArgs {
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Clone)]
pub struct ProfileSaveArgs {
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long = "stdin-json")]
    pub stdin_json: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Clone)]
pub struct ProfileClearArgs {
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Clone, Default)]
pub struct SharedRunArgs {
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long, conflicts_with = "hw_key")]
    pub seed: Option<String>,
    #[arg(long, conflicts_with = "seed")]
    pub hw_key: Option<String>,
    #[arg(long)]
    pub kdf_label: Option<String>,
    #[arg(long)]
    pub server_url: Option<String>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Clone)]
pub struct InfoArgs {
    #[command(flatten)]
    pub shared: SharedRunArgs,
}

#[derive(Args, Clone)]
pub struct ProvisionArgs {
    #[command(flatten)]
    pub shared: SharedRunArgs,
    #[arg(long = "num-keys")]
    pub num_keys: Option<u32>,
}

#[derive(Args, Clone)]
pub struct KeyboxArgs {
    #[command(flatten)]
    pub shared: SharedRunArgs,
    #[arg(long)]
    pub output: Option<String>,
}

#[derive(Args, Clone)]
pub struct VerifyArgs {
    pub csr_file: String,
    #[arg(long)]
    pub json: bool,
}
