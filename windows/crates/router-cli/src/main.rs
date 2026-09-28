//! A CLI `router` que o app empacota e que faz a ponte com o terminal.
//!
//!   router statusline      — o sensor: lê `rate_limits` do stdin e grava por
//!                            conta. Nenhuma chamada de rede, nenhum token nosso.
//!   router launch <grupo>  — ativa a melhor conta do grupo e sobe o `claude`.
//!   router is-group <nome> — código 0 se `<nome>` é um grupo (para o shell).
//!   router rotate          — uma volta da rotação (para um agendador).
//!   router doctor          — confere a instalação e diz o que está torto.
//!   router measure         — a SONDA: pergunta o uso ao binário oficial,
//!                            inclusive o limite POR MODELO que o sensor não vê.
//!   router shim <args>     — o `claude` do Prompt de Comando: grupo → `launch`,
//!                            resto → o claude de verdade. O cmd não tem perfil
//!                            nem função de shell, então a decisão vem para cá.
//!   router uninstall-integration — desfaz o que a integração escreveu (AutoRun,
//!                            `$PROFILE`, `.bashrc`).
//!
//! Sem argumento, o comando é `statusline` — igual ao macOS. Tudo best-effort do
//! lado do sensor (não pode travar a status line); o resto reporta erro e sai
//! com código diferente de zero.

mod doctor;
mod launch;
mod measure;
mod shared;
mod shim;
mod statusline;
mod uninstall;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("statusline");
    let rest = args.get(1..).unwrap_or_default();

    match command {
        "statusline" => statusline::run(rest),
        "launch" => launch::run(rest),
        "is-group" => std::process::exit(if launch::is_group(rest.first().map(String::as_str)) {
            0
        } else {
            1
        }),
        "rotate" => launch::rotate(),
        "doctor" => std::process::exit(if doctor::run() { 0 } else { 1 }),
        "measure" => std::process::exit(if measure::run(rest) { 0 } else { 1 }),
        "shim" => shim::run(rest),
        "uninstall-integration" => std::process::exit(if uninstall::run() { 0 } else { 1 }),
        _ => {
            // Os nomes novos vão no FIM: o teste da linha de uso compara por
            // prefixo (`cli_tests.rs`), e inserir no meio o quebraria à toa.
            eprintln!(
                "uso: router [statusline|launch <grupo>|is-group <nome>|rotate|doctor|measure [grupo]|shim <args>|uninstall-integration]"
            );
            std::process::exit(2);
        }
    }
}
