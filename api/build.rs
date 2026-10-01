fn main() {
    // embed_migrations! cannot see a new migration directory on its own, so an
    // incremental build would ship a binary without it.
    // ref: https://docs.rs/diesel_migrations/2.2.0/diesel_migrations/macro.embed_migrations.html#automatic-rebuilds
    println!("cargo:rerun-if-changed=migrations");
}
