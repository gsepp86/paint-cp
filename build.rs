// Compila le risorse di Windows (icona dell'eseguibile) quando si costruisce per Windows.
// Il controllo usa la variabile di Cargo invece di `cfg!`, cosi' il file viene
// controllato dal compilatore anche su Linux e macOS.

fn main() {
    println!("cargo:rerun-if-changed=paint_cp.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("paint_cp.rc", embed_resource::NONE);
    }
}
