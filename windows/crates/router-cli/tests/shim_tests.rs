//! O `router shim` ponta a ponta — o `claude` do Prompt de Comando. Prova a
//! bifurcação: nome de grupo vai para o `launch`; qualquer outra coisa vai para
//! o `claude` de verdade com o ambiente INTOCADO. Tudo no sandbox, com o
//! `fake-claude`; nenhuma conta real.

mod common;
use common::*;

/// O caso que motivou tudo: no cmd, `claude trabalho` tem de ativar a conta do
/// grupo e subir no perfil dele — e não virar o primeiro prompt da sessão
/// padrão, queimando a cota do grupo errado.
#[test]
fn a_group_name_launches_the_session_in_the_group_profile() {
    let w = world(2);

    let out = w.sandbox.run(&["shim", "trabalho", "--resume", "abc"]);

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("→ Trabalho: conta1"),
        "{}",
        stderr(&out)
    );
    let runs = w.sandbox.records();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["args"], serde_json::json!(["--resume", "abc"]));
    assert_eq!(
        runs[0]["claudeConfigDir"].as_str(),
        Some(w.group.config_dir.raw.as_str())
    );
    assert_eq!(w.group_identity().as_deref(), Some("conta1@exemplo.com"));
}

/// Não é grupo → o claude de verdade, como se o router não existisse: o
/// argumento chega intacto e nenhum `CLAUDE_CONFIG_DIR` é acrescentado.
#[test]
fn anything_else_reaches_the_real_claude_untouched() {
    let w = world(1);

    let out = w.sandbox.run(&["shim", "--version"]);

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("2.1.280 (Claude Code)"),
        "{}",
        stdout(&out)
    );
    let runs = w.sandbox.records();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["args"], serde_json::json!(["--version"]));
    assert!(runs[0]["claudeConfigDir"].is_null(), "{}", runs[0]);
    // Nada foi ativado: nenhuma credencial no perfil do grupo.
    assert!(!w.group_dir().join(".credentials.json").exists());
}

/// Só `claude`, sem argumento: também é o claude de verdade.
#[test]
fn no_arguments_at_all_is_the_real_claude_too() {
    let w = world(1);

    let out = w.sandbox.run(&["shim"]);

    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let runs = w.sandbox.records();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["args"], serde_json::json!([]));
}

/// O Windows não tem `exec`: o router espera o filho, então o código de saída
/// dele precisa ser devolvido ao cmd (`if errorlevel` do usuário depende disso).
#[test]
fn the_exit_code_of_the_real_claude_is_propagated() {
    let w = world(1);

    let out = w
        .sandbox
        .router(&["shim", "--version"])
        .env("FAKE_CLAUDE_EXIT", "7")
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(7), "{}", stderr(&out));
}

/// Sem binário oficial não há o que rodar: erro claro e código != 0.
#[test]
fn a_missing_binary_is_named() {
    let sandbox = Sandbox::new();

    let out = sandbox
        .router(&["shim", "--version"])
        .env("ROUTER_CLAUDE_BIN", sandbox.home.join("nao-existe.exe"))
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("router: não foi possível executar claude: binário não encontrado"),
        "{}",
        stderr(&out)
    );
}
