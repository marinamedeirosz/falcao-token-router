//! O laço do app enquanto ele vive (≙ `startRotation` do macOS).
//!
//! A cada 30 s relê o quadro (amostras do sensor, conta ativa, sessões vivas) e
//! redesenha a bandeja; a cada 180 s também roda a volta de rotação — espelha a
//! ativa de cada grupo e troca se ela passou do limiar. Três minutos para a
//! rotação porque o `rate_limits` só muda quando há atividade: reconsultar mais
//! rápido não traz número novo, só trabalho. A releitura é barata (arquivos
//! pequenos) e deixa o número da bandeja acompanhar o sensor.

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::login::LoginState;
use crate::state::AppState;
use crate::tray;

/// De quanto em quanto tempo a bandeja relê o quadro.
const REFRESH_EVERY: Duration = Duration::from_secs(30);
/// A rotação roda a cada tantas releituras (6 × 30 s = 180 s, como no macOS).
const ROTATE_EVERY_TICKS: u64 = 6;
/// Por quanto tempo, no máximo, um login segura a rotação.
const LOGIN_HOLD_MAX: Duration = Duration::from_secs(10 * 60);

/// O evento que as janelas ouvem para reler o quadro.
pub const SNAPSHOT_CHANGED: &str = "snapshot-changed";

/// Desde quando um login vem segurando a rotação (`None` = não segura).
static LOGIN_HOLD_SINCE: Mutex<Option<Instant>> = Mutex::new(None);

/// Uma volta: relê (e, se `rotate`, roda a rotação), redesenha, avisa as janelas.
/// Com um login em andamento a rotação fica para a próxima volta — até o prazo
/// (ver `login_holds`).
pub fn tick(app: &AppHandle, rotate: bool) {
    let rotate = rotate && !login_holds(app);
    {
        let state = app.state::<AppState>();
        let mut store = state.store();
        store.refresh_usage();
        if rotate {
            store.rotate_all();
        }
    }
    tray::refresh(app);
    let _ = app.emit(SNAPSHOT_CHANGED, ());
}

/// A decisão de segurar, separada do relógio para poder ser testada: `since` é
/// desde quando o login vem segurando, e zera quando não há login.
fn holds(running: bool, since: &mut Option<Instant>, now: Instant) -> bool {
    if !running {
        *since = None;
        return false;
    }
    now.saturating_duration_since(*since.get_or_insert(now)) < LOGIN_HOLD_MAX
}

/// Um login em andamento segura a rotação (o porquê está em
/// `LoginState::in_progress`: a volta espelha grupo → casa da conta ativa e
/// pisaria na credencial que o `claude auth login` acabou de gravar) — mas só
/// até o prazo. A guarda não tinha fim: um diálogo aberto e esquecido, ou um
/// login que nunca termina, congelava o rodízio de TODOS os grupos por tempo
/// indeterminado, e nada na tela explicava — as contas só paravam de girar.
///
/// Estreitar por GRUPO seria o corte exato, mas nem o `LoginFlow` diz de que
/// grupo é o login nem o núcleo sabe rodar a volta sem um deles; o prazo cabe
/// aqui e resolve o caso real (o login que trava é raro; o esquecido, não).
/// Risco aceito: um login que passe de 10 min pode ter a credencial recém-
/// gravada pisada pelo espelho do grupo e precisar ser refeito — caro, mas
/// visível, ao contrário do rodízio parado em silêncio. A conta corre nas
/// voltas de rotação, então o prazo vence na primeira volta depois dele.
fn login_holds(app: &AppHandle) -> bool {
    let running = app
        .try_state::<LoginState>()
        .is_some_and(|login| login.in_progress());
    let mut since = LOGIN_HOLD_SINCE.lock().unwrap_or_else(|p| p.into_inner());
    holds(running, &mut since, Instant::now())
}

pub fn start(app: AppHandle) {
    let spawned = thread::Builder::new()
        .name("rotacao".into())
        .spawn(move || {
            // Como no macOS, a primeira volta (com rotação) é já na subida.
            let mut count: u64 = 0;
            loop {
                tick(&app, count.is_multiple_of(ROTATE_EVERY_TICKS));
                count += 1;
                thread::sleep(REFRESH_EVERY);
            }
        });
    if let Err(e) = spawned {
        eprintln!("falcao: o laço de rotação não subiu: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forgotten_login_stops_holding_rotation_after_the_deadline() {
        let start = Instant::now();
        let mut since = None;
        assert!(
            holds(true, &mut since, start),
            "o login recém-aberto segura"
        );
        assert!(holds(
            true,
            &mut since,
            start + LOGIN_HOLD_MAX - Duration::from_secs(1)
        ));
        assert!(
            !holds(true, &mut since, start + LOGIN_HOLD_MAX),
            "passado o prazo, o rodízio volta a girar"
        );
    }

    #[test]
    fn without_a_login_it_never_holds_and_the_clock_restarts() {
        let start = Instant::now();
        let mut since = None;
        assert!(holds(true, &mut since, start));
        assert!(!holds(false, &mut since, start + Duration::from_secs(60)));
        assert_eq!(since, None, "o login que terminou não deixa relógio atrás");
        // O login seguinte tem o prazo inteiro, não o que sobrou do anterior.
        let later = start + LOGIN_HOLD_MAX;
        assert!(holds(true, &mut since, later));
        assert!(holds(
            true,
            &mut since,
            later + LOGIN_HOLD_MAX - Duration::from_secs(1)
        ));
    }
}
