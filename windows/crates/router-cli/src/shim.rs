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

use router_core::platform::console;
use router_core::usage::claude_binary::ClaudeBinary;

use crate::launch;
use crate::shared::fail;

pub fn run(argv: &[String]) {
    // Mesma regra do `is-group` que a função de shell consulta: sem caixa e sem
    // espaço nas pontas. `launch::run` faz o resto (trava, ativação, sensor,
    // ambiente, código de saída) e não volta.
    if launch::is_group(argv.first().map(String::as_str)) {
        launch::run(argv);
        return;
    }

    let Some(claude) = ClaudeBinary::locate() else {
        fail("não foi possível executar claude: binário não encontrado");
    };
    // Sem `env_clear`, sem tirar nem pôr `CLAUDE_CONFIG_DIR`: quem digitou
    // `claude` puro no cmd tem de receber a sessão que receberia sem o router —
    // inclusive dentro de um perfil que já esteja no ambiente.
    console::ignore_interrupts_in_this_process();
    match claude.command().args(argv).status() {
        // O Windows não tem `exec`: o router espera o filho e devolve o código.
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => fail(&format!("não foi possível executar claude: {e}")),
    }
}
