use clap;

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum Network {
    Mainnet,
    Signet,
}
