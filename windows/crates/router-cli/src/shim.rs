//! `router shim` — a decisão de roteamento em Rust, uma vez só.
//!
//! O `shell.ps1` e o `shell.sh` tomam essa decisão em shell script porque
//! precisam ENCADEAR uma função `claude` que o usuário já tivesse no perfil. O
//! Prompt de Comando não tem função nem perfil: a macro `doskey` só sabe chamar
//! um programa. Então quem decide é o router.
//!
//! `claude <grupo>` → `launch`; qualquer outra coisa → o `claude` de verdade,
//! com o ambiente INTOCADO (sem `CLAUDE_CONFIG_DIR`), para o `claude` puro do
//! cmd se comportar exatamente como se o router não existisse.

/// TRILHA T4: `launch::is_group(argv.first())` → `launch::run(argv)`; senão
/// `ClaudeBinary::locate()` e roda com o ambiente herdado, saindo com o código
/// do filho (o padrão de `launch.rs`, inclusive o `ignore_interrupts_in_this_process`).
pub fn run(_argv: &[String]) {
    eprintln!("router shim: ainda não implementado");
    std::process::exit(2);
}
