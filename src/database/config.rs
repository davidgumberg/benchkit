use anyhow::{bail, Context, Result};
use log::{debug, info};
use postgres::{Client, NoTls};
use serde::Deserialize;
use std::{io, process::Command};

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: usize,
    pub database: String,
    pub user: String,
    pub password: String,
}

impl DatabaseConfig {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.database
        )
    }
}

pub fn initialize_database(db_conf: &DatabaseConfig) -> Result<()> {
    info!("Initializing database...");
    check_postgres_running()?;

    let user_exists = check_postgres_user(&db_conf.user)?;
    if !user_exists {
        create_postgres_user(&db_conf.user, &db_conf.password)?;
    }
    ensure_user_createdb(&db_conf.user)?;

    let db_exists = check_postgres_database(&db_conf.database)?;
    if !db_exists {
        create_postgres_database(&db_conf.database, &db_conf.user)?;
        grant_privileges(&db_conf.database, &db_conf.user)?;
    }

    let mut client = Client::connect(&db_conf.connection_string(), NoTls)?;
    client.batch_execute(include_str!("schema.sql"))?;
    info!("Database setup completed successfully");
    Ok(())
}

fn check_postgres_running() -> Result<()> {
    let status = Command::new("pg_isready").status()?;
    if !status.success() {
        anyhow::bail!("PostgreSQL is not running");
    }
    Ok(())
}

fn check_postgres_user(user: &str) -> Result<bool> {
    let output = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-tAc")
        .arg(format!("SELECT 1 FROM pg_roles WHERE rolname = '{}'", user))
        .output()
        .with_context(|| "Failed to execute postgres user check command")?;

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn ensure_user_createdb(user: &str) -> Result<()> {
    let has_createdb = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-tAc")
        .arg(format!(
            "SELECT 1 FROM pg_roles WHERE rolname = '{}' AND rolcreatedb",
            user
        ))
        .output()
        .with_context(|| "Failed to check CREATEDB permission")?;

    if String::from_utf8_lossy(&has_createdb.stdout)
        .trim()
        .is_empty()
    {
        info!("Granting CREATEDB permission to {}", user);
        let status = Command::new("sudo")
            .arg("-u")
            .arg("postgres")
            .arg("psql")
            .arg("-c")
            .arg(format!("ALTER USER \"{}\" CREATEDB", user))
            .status()
            .with_context(|| "Failed to grant CREATEDB permission")?;

        if !status.success() {
            anyhow::bail!("Failed to grant CREATEDB permission to {}", user);
        }
    }

    Ok(())
}

fn check_postgres_database(database: &str) -> Result<bool> {
    let output = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-tAc")
        .arg(format!(
            "SELECT 1 FROM pg_database WHERE datname = '{}'",
            database
        ))
        .output()
        .with_context(|| "Failed to execute postgres database check command")?;

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn create_postgres_user(user: &str, password: &str) -> Result<()> {
    info!("Creating user {}", user);
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "CREATE USER \"{}\" WITH PASSWORD '{}'",
            user, password
        ))
        .status()
        .with_context(|| "Failed to execute create user command")?;

    if !status.success() {
        anyhow::bail!("Failed to create user {}", user);
    }
    Ok(())
}

fn create_postgres_database(database: &str, owner: &str) -> Result<()> {
    info!("Creating database {}", database);
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "CREATE DATABASE \"{}\" WITH OWNER = \"{}\"",
            database, owner
        ))
        .status()
        .with_context(|| "Failed to execute create database command")?;

    if !status.success() {
        anyhow::bail!("Failed to create database {}", database);
    }
    Ok(())
}

fn grant_privileges(database: &str, user: &str) -> Result<()> {
    info!("Granting privileges on {} to {}", database, user);
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "GRANT ALL PRIVILEGES ON DATABASE \"{}\" TO \"{}\"",
            database, user
        ))
        .status()
        .with_context(|| "Failed to execute grant privileges command")?;

    if !status.success() {
        anyhow::bail!("Failed to grant privileges on {} to {}", database, user);
    }
    Ok(())
}

pub fn delete_database_interactive(db_config: &DatabaseConfig) -> Result<()> {
    println!("⚠️  WARNING: You are about to delete:");
    println!("  Database: {}", db_config.database);
    println!("  User: {}", db_config.user);
    println!("  Host: {}:{}", db_config.host, db_config.port);
    println!("\nAre you sure? Type 'yes' to confirm: ");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() == "yes" {
        delete_database(db_config)?;
    } else {
        bail!("Database deletion cancelled.");
    }
    Ok(())
}

fn delete_database(config: &DatabaseConfig) -> Result<()> {
    info!("Deleting database...");
    check_postgres_running()?;

    if check_postgres_database(&config.database)? {
        info!("Dropping database {}", config.database);
        drop_database(&config.database)?;
    } else {
        info!("Database {} does not exist", config.database);
    }

    if check_postgres_user(&config.user)? {
        info!("Dropping user {}", config.user);
        drop_user(&config.user)?;
    } else {
        info!("User {} does not exist", config.user);
    }
    info!("Database and user deleted successfully.");
    Ok(())
}

fn drop_database(database: &str) -> Result<()> {
    // First terminate all connections to the database
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
            database
        ))
        .status()
        .with_context(|| "Failed to terminate database connections")?;

    if !status.success() {
        anyhow::bail!("Failed to terminate database connections");
    }

    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!("DROP DATABASE \"{}\"", database))
        .status()
        .with_context(|| format!("Failed to drop database {}", database))?;

    if !status.success() {
        anyhow::bail!("Failed to drop database {}", database);
    }
    Ok(())
}

fn drop_user(user: &str) -> Result<()> {
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!("DROP USER \"{}\"", user))
        .status()
        .with_context(|| format!("Failed to drop user {}", user))?;

    if !status.success() {
        anyhow::bail!("Failed to drop user {}", user);
    }
    Ok(())
}

pub fn check_connection(db_conf: &DatabaseConfig) -> Result<()> {
    debug!("Testing connection to postgres db");
    let mut client = Client::connect(&db_conf.connection_string(), NoTls)?;
    client
        .execute("SELECT 1", &[])
        .with_context(|| "Failed to execute test query")?;
    info!("Successfully connected to database");

    Ok(())
}
