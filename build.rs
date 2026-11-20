// Compila le risorse di Windows (icona dell'eseguibile) quando si costruisce su Windows.

fn main() {
    println!("cargo:rerun-if-changed=paint_cp.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    #[cfg(windows)]
    {
        embed_resource::compile("paint_cp.rc");
    }
}
