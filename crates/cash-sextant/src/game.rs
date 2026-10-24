//! The game: a ship at the bottom, a formation of enemies that sways at the top and sends
//! some of its members down in dives, shots both ways, explosions, and stars behind.
//! Positions are in canvas pixels, times in seconds.

use crossterm::event::KeyCode;
use std::time::Instant;

use crate::canvas::{Canvas, Rgb};
use crate::input::Keys;
use crate::sprites::{DRONE, FLAME, PLAYER, WARDEN};

const COLS: usize = 8;
const ROWS: usize = 4;
const SPACING_X: f32 = 14.0;
const SPACING_Y: f32 = 9.0;
const PLAYER_SPEED: f32 = 120.0;
const SHOT_SPEED: f32 = 200.0;
const SHOT_COOLDOWN: f32 = 0.16;
const MAX_SHOTS: usize = 4;
const ENEMY_SHOT_SPEED: f32 = 85.0;
const RESPAWN: f32 = 1.5;

/// A small xorshift generator: enough for a game, and no dependency.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number in `0.0..1.0`.
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Drone,
    Warden,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Formation,
    /// Diving since `t` seconds, from where it left, swinging to the side `dir`.
    Diving {
        t: f32,
        from: (f32, f32),
        dir: f32,
    },
    /// Back in from the top, flying to its place.
    Returning,
}

struct Enemy {
    kind: Kind,
    slot: (usize, usize),
    state: State,
    x: f32,
    y: f32,
    alive: bool,
}

struct Shot {
    x: f32,
    y: f32,
    vy: f32,
}

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
}

struct Star {
    x: f32,
    y: f32,
    layer: usize,
}

const STAR_SPEED: [f32; 3] = [5.0, 12.0, 26.0];
const STAR_COLOUR: [Rgb; 3] = [0x0030_3848, 0x0058_6078, 0x0090_98b8];

pub(crate) struct Game {
    width: f32,
    height: f32,
    t: f32,
    rng: Rng,
    player_x: f32,
    /// Seconds until the ship is back, while it is gone.
    respawn: Option<f32>,
    /// Seconds the ship cannot be hit, blinking.
    shielded: f32,
    cooldown: f32,
    shots: Vec<Shot>,
    enemy_shots: Vec<Shot>,
    enemies: Vec<Enemy>,
    particles: Vec<Particle>,
    stars: Vec<Star>,
    next_dive: f32,
    /// How far above its place the formation still is, as a wave flies in.
    entry: f32,
    pub(crate) score: u32,
    pub(crate) wave: u32,
    pub(crate) lives: u32,
    pub(crate) over: bool,
}

impl Game {
    pub(crate) fn new(width: usize, height: usize, seed: u64) -> Self {
        let mut game = Self {
            width: width as f32,
            height: height as f32,
            t: 0.0,
            rng: Rng(seed | 1),
            player_x: width as f32 / 2.0,
            respawn: None,
            shielded: 0.0,
            cooldown: 0.0,
            shots: Vec::new(),
            enemy_shots: Vec::new(),
            enemies: Vec::new(),
            particles: Vec::new(),
            stars: Vec::new(),
            next_dive: 3.0,
            entry: 0.0,
            score: 0,
            wave: 0,
            lives: 3,
            over: false,
        };
        game.scatter_stars();
        game.next_wave();
        game
    }

    pub(crate) fn resize(&mut self, width: usize, height: usize) {
        self.width = width as f32;
        self.height = height as f32;
        self.player_x = self.player_x.clamp(6.0, self.width - 6.0);
        self.scatter_stars();
    }

    fn scatter_stars(&mut self) {
        let count = (self.width * self.height / 90.0) as usize;
        self.stars = (0..count)
            .map(|i| Star {
                x: self.rng.range(0.0, self.width),
                y: self.rng.range(0.0, self.height),
                layer: i % 3,
            })
            .collect();
    }

    fn next_wave(&mut self) {
        self.wave += 1;
        self.enemies = (0..ROWS)
            .flat_map(|r| (0..COLS).map(move |c| (c, r)))
            .map(|slot| Enemy {
                kind: if slot.1 == 0 {
                    Kind::Warden
                } else {
                    Kind::Drone
                },
                slot,
                state: State::Formation,
                x: 0.0,
                y: 0.0,
                alive: true,
            })
            .collect();
        self.entry = 60.0;
        self.next_dive = 3.0;
    }

    fn player_y(&self) -> f32 {
        self.height - 6.0
    }

    /// Where the enemy in `slot` belongs now: the formation sways and breathes.
    fn place(&self, slot: (usize, usize)) -> (f32, f32) {
        let room = ((self.width - COLS as f32 * SPACING_X) / 2.0 - 6.0).clamp(0.0, 30.0);
        let sway = (self.t * 0.7).sin() * room;
        let breath = 1.0 + 0.08 * (self.t * 1.3).sin();
        let x = self.width / 2.0 + sway + (slot.0 as f32 - 3.5) * SPACING_X * breath;
        let y = 8.0 + slot.1 as f32 * SPACING_Y - self.entry;
        (x, y)
    }

    pub(crate) fn restart(&mut self) {
        let (w, h) = (self.width as usize, self.height as usize);
        let seed = self.rng.next();
        *self = Self::new(w, h, seed);
    }

    /// Steering for `--demo`: under the lowest enemy, firing always.
    pub(crate) fn autopilot(&self) -> (f32, bool) {
        let target = self
            .enemies
            .iter()
            .filter(|e| e.alive)
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .map_or(self.width / 2.0, |e| e.x);
        let steer = ((target - self.player_x) / 8.0).clamp(-1.0, 1.0);
        (steer, true)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one frame of the game in order: stars, ship, dives, enemies, shots, hits"
    )]
    pub(crate) fn update(&mut self, dt: f32, keys: &Keys, now: Instant, demo: bool) {
        self.t += dt;
        self.entry = (self.entry - 40.0 * dt).max(0.0);

        for star in &mut self.stars {
            star.y += STAR_SPEED[star.layer] * dt;
            if star.y >= self.height {
                star.y -= self.height;
                star.x = self.rng.range(0.0, self.width);
            }
        }

        // The ship.
        let (steer, fire) = if demo {
            self.autopilot()
        } else {
            let left = keys.held(KeyCode::Left, now) || keys.held(KeyCode::Char('a'), now);
            let right = keys.held(KeyCode::Right, now) || keys.held(KeyCode::Char('d'), now);
            let fire = keys.held(KeyCode::Char(' '), now) || keys.held(KeyCode::Up, now);
            (f32::from(i8::from(right) - i8::from(left)), fire)
        };
        self.cooldown -= dt;
        self.shielded = (self.shielded - dt).max(0.0);
        match self.respawn {
            Some(left) if left - dt <= 0.0 => {
                self.respawn = None;
                self.shielded = 1.5;
                self.player_x = self.width / 2.0;
            }
            Some(left) => self.respawn = Some(left - dt),
            None if !self.over => {
                self.player_x =
                    (self.player_x + steer * PLAYER_SPEED * dt).clamp(6.0, self.width - 6.0);
                if fire && self.cooldown <= 0.0 && self.shots.len() < MAX_SHOTS {
                    self.shots.push(Shot {
                        x: self.player_x,
                        y: self.player_y() - 4.0,
                        vy: -SHOT_SPEED,
                    });
                    self.cooldown = SHOT_COOLDOWN;
                }
            }
            None => {}
        }

        // Dives: one now and then, two later in the game.
        self.next_dive -= dt;
        if self.next_dive <= 0.0 && self.entry == 0.0 {
            let speedup = 1.0 + 0.15 * self.wave as f32;
            self.next_dive = self.rng.range(1.2, 2.6) / speedup;
            let divers = if self.wave > 2 && self.rng.unit() < 0.4 {
                2
            } else {
                1
            };
            for _ in 0..divers {
                let waiting: Vec<usize> = (0..self.enemies.len())
                    .filter(|&i| self.enemies[i].alive && self.enemies[i].state == State::Formation)
                    .collect();
                if waiting.is_empty() {
                    break;
                }
                let i = waiting[(self.rng.next() % waiting.len() as u64) as usize];
                let dir = if self.enemies[i].x < self.width / 2.0 {
                    1.0
                } else {
                    -1.0
                };
                let from = (self.enemies[i].x, self.enemies[i].y);
                self.enemies[i].state = State::Diving { t: 0.0, from, dir };
            }
        }

        // The enemies.
        let player_x = self.player_x;
        let speedup = 1.0 + 0.1 * self.wave as f32;
        for i in 0..self.enemies.len() {
            if !self.enemies[i].alive {
                continue;
            }
            let home = self.place(self.enemies[i].slot);
            let enemy = &mut self.enemies[i];
            match enemy.state {
                State::Formation => (enemy.x, enemy.y) = home,
                State::Diving { t, from, dir } => {
                    let t = t + dt * speedup;
                    enemy.y = from.1 + 26.0 * t + 14.0 * t * t;
                    let swing = from.0 + dir * 34.0 * (t * 2.0).sin();
                    enemy.x = swing + (player_x - swing) * (t / 3.0).min(0.6);
                    enemy.state = State::Diving { t, from, dir };
                    if enemy.y > self.height + 8.0 {
                        enemy.state = State::Returning;
                        enemy.x = home.0;
                        enemy.y = -8.0;
                    } else if enemy.y < self.height * 0.75 && self.rng.unit() < 0.9 * dt * speedup {
                        self.enemy_shots.push(Shot {
                            x: enemy.x,
                            y: enemy.y + 4.0,
                            vy: ENEMY_SHOT_SPEED * speedup,
                        });
                    }
                }
                State::Returning => {
                    let (dx, dy) = (home.0 - enemy.x, home.1 - enemy.y);
                    let distance = dx.hypot(dy);
                    let step = 50.0 * dt;
                    if distance <= step {
                        enemy.state = State::Formation;
                        (enemy.x, enemy.y) = home;
                    } else {
                        enemy.x += dx / distance * step;
                        enemy.y += dy / distance * step;
                    }
                }
            }
        }

        // Shots, and what they hit.
        for shot in self.shots.iter_mut().chain(self.enemy_shots.iter_mut()) {
            shot.y += shot.vy * dt;
        }
        let height = self.height;
        self.shots.retain(|s| s.y > -3.0);
        self.enemy_shots.retain(|s| s.y < height + 3.0);

        let mut hits = Vec::new();
        self.shots.retain(|shot| {
            let hit = self.enemies.iter().position(|e| {
                e.alive && (shot.x - e.x).abs() <= 5.5 && (shot.y - e.y).abs() <= 3.5
            });
            if let Some(i) = hit {
                hits.push(i);
            }
            hit.is_none()
        });
        for i in hits {
            let (x, y, kind, state) = {
                let e = &self.enemies[i];
                (e.x, e.y, e.kind, e.state)
            };
            if !self.enemies[i].alive {
                continue;
            }
            self.enemies[i].alive = false;
            let diving = matches!(state, State::Diving { .. });
            self.score += match (kind, diving) {
                (Kind::Drone, false) => 100,
                (Kind::Drone, true) => 200,
                (Kind::Warden, false) => 150,
                (Kind::Warden, true) => 400,
            };
            self.explode(x, y, 16);
        }

        if self.respawn.is_none() && !self.over && self.shielded == 0.0 {
            let (px, py) = (self.player_x, self.player_y());
            let shot_hit = self
                .enemy_shots
                .iter()
                .position(|s| (s.x - px).abs() <= 4.5 && (s.y - py).abs() <= 3.0);
            let ram = self
                .enemies
                .iter()
                .position(|e| e.alive && (e.x - px).abs() <= 8.0 && (e.y - py).abs() <= 5.0);
            if let Some(i) = shot_hit {
                self.enemy_shots.swap_remove(i);
            }
            if let Some(i) = ram {
                self.enemies[i].alive = false;
                let (x, y) = (self.enemies[i].x, self.enemies[i].y);
                self.explode(x, y, 12);
            }
            if shot_hit.is_some() || ram.is_some() {
                self.explode(px, py, 40);
                self.lives = self.lives.saturating_sub(1);
                if self.lives == 0 {
                    self.over = true;
                } else {
                    self.respawn = Some(RESPAWN);
                }
            }
        }

        for p in &mut self.particles {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy += 30.0 * dt;
            p.age += dt;
        }
        self.particles.retain(|p| p.age < p.life);

        if self.enemies.iter().all(|e| !e.alive) {
            self.enemy_shots.clear();
            self.next_wave();
        }
    }

    fn explode(&mut self, x: f32, y: f32, count: usize) {
        for _ in 0..count {
            let angle = self.rng.range(0.0, std::f32::consts::TAU);
            let speed = self.rng.range(10.0, 60.0);
            let life = self.rng.range(0.3, 0.9);
            self.particles.push(Particle {
                x,
                y,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed * 0.6,
                age: 0.0,
                life,
            });
        }
    }

    pub(crate) fn draw(&self, canvas: &mut Canvas) {
        for star in &self.stars {
            canvas.set(star.x as i32, star.y as i32, STAR_COLOUR[star.layer]);
        }
        let frame = (self.t * 2.5) as usize % 2;
        for e in self.enemies.iter().filter(|e| e.alive) {
            let sprite = match e.kind {
                Kind::Drone => &DRONE[frame],
                Kind::Warden => &WARDEN[frame],
            };
            sprite.draw(canvas, e.x, e.y, None);
        }
        for s in &self.shots {
            for dy in 0..3 {
                canvas.set(s.x.round() as i32, s.y.round() as i32 + dy, 0x00ff_f0a0);
            }
        }
        for s in &self.enemy_shots {
            canvas.set(s.x.round() as i32, s.y.round() as i32, 0x00ff_6030);
            canvas.set(s.x.round() as i32, s.y.round() as i32 + 1, 0x00ff_2020);
        }
        let blink = self.shielded > 0.0 && (self.t * 10.0) as i32 % 2 == 0;
        if self.respawn.is_none() && !self.over && !blink {
            PLAYER.draw(canvas, self.player_x, self.player_y(), None);
            if (self.t * 15.0) as i32 % 2 == 0 {
                FLAME.draw(canvas, self.player_x, self.player_y() + 4.5, None);
            }
        }
        for p in &self.particles {
            let colour = match p.age / p.life {
                a if a < 0.2 => 0x00ff_ffff,
                a if a < 0.45 => 0x00ff_e040,
                a if a < 0.7 => 0x00ff_8020,
                _ => 0x00a0_2010,
            };
            canvas.set(p.x.round() as i32, p.y.round() as i32, colour);
        }
    }
}
