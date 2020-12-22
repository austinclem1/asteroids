extern crate nalgebra_glm as glm;
extern crate sdl2;

use glm::Vec2;
use rand::Rng;
use std::path::Path;
use sdl2::pixels::Color;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::video::Window;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas};
use sdl2::rwops::RWops;
use std::time::{Duration, Instant};
use sdl2::mixer::LoaderRWops;
use rust_embed::RustEmbed;

#[macro_use]
use strum_macros::{EnumString, EnumVariantNames};
use strum::VariantNames;

#[derive(RustEmbed)]
#[folder = "./assets/"]
struct Asset;

const SCREEN_WIDTH: u32 = 640;
const SCREEN_HEIGHT: u32 = 480;

const ASTEROID_SPAWN_TIME: f32 = 5.0;

thread_local! {
    static SOUNDS: Vec<sdl2::mixer::Chunk> = load_all_sounds().unwrap();
}

macro_rules! rect (
    ($x:expr, $y:expr, $w:expr, $h:expr) => (
        Rect::new($x as i32, $y as i32, $w as u32, $h as u32)
    )
);
macro_rules! rect_from_center (
    ($p:expr, $w:expr, $h:expr) => (
        Rect::from_center(($p.x as i32, $p.y as i32), $w as u32, $h as u32)
    )
);

pub fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    // let _audio_subsystem = sdl_context.audio()?;
    let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;

    let font_path = Path::new("./assets/fonts/Open 24 Display St.ttf");
    let font_data = include_bytes!("../assets/fonts/Open 24 Display St.ttf");
    let mut font = ttf_context.load_font_from_rwops(RWops::from_bytes(font_data)?, 64)?;
    font.set_style(sdl2::ttf::FontStyle::NORMAL);

    {
        let sample_rate = 44100;
        let audio_format = sdl2::mixer::AUDIO_S16SYS;
        let channels = 2;
        let chunk_size = 256;
        sdl2::mixer::open_audio(sample_rate, audio_format, channels, chunk_size)?;
        sdl2::mixer::allocate_channels(4);
    }

    // This will initialize SOUNDS
    SOUNDS.with(|_| {});

    let window = video_subsystem.window("Asteroids", SCREEN_WIDTH, SCREEN_HEIGHT)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;
    
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    canvas.set_blend_mode(BlendMode::Blend);
    let texture_creator = canvas.texture_creator();

    canvas.set_draw_color(Color::RGB(0, 64, 255));
    canvas.clear();
    canvas.present();
    let mut event_pump = sdl_context.event_pump()?;
    let mut paused_instant = Instant::now();
    let mut last_frame_time = Instant::now();
    let mut this_frame_time;
    let mut delta;

    let mut score = 0;

    let mut player = Player::new(320.0, 240.0);
    let mut bullets = Vec::new();
    let mut queued_bullet_deletion_indices = Vec::new();

    let mut asteroids = Vec::new();
    asteroids.push(Asteroid::new(
            glm::vec2(100.0, 100.0),
            glm::vec2(10.0, 30.0),
            30
            ));
    asteroids.push(spawn_asteroid());
    let mut queued_asteroid_deletion_indices = Vec::new();
    let mut last_asteroid_spawn_time = Instant::now();

    let mut particles = Vec::new();

    'running: loop {
        this_frame_time = Instant::now();
        delta = this_frame_time.duration_since(last_frame_time).as_secs_f32();
        last_frame_time = this_frame_time;
        canvas.set_draw_color(Color::BLACK);
        canvas.clear();
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit {..} => {
                    break 'running
                },
                // Event::Window { win_event: WindowEvent::FocusLost, .. } => {
                //     paused_instant = Instant::now();
                // },
                // Event::Window { win_event: WindowEvent::FocusGained, .. } => {
                //     last_frame_time += Instant::now().duration_since(paused_instant);
                // },
                Event::KeyDown { scancode: Some(Scancode::Space), repeat: false, .. } if player.is_alive => {
                    bullets.push(player.spawn_bullet());
                    play_sound(Sound::Shoot);
                },
                Event::KeyDown { scancode: Some(Scancode::R), .. } if !player.is_alive => {
                    player.is_alive = true;

                },
                _ => {}
            }
        }
        // player movement controls
        if player.is_alive {
            for scancode in event_pump.keyboard_state().pressed_scancodes() {
                match scancode {
                    Scancode::Up => {
                        player.accelerate(delta);
                    },
                    Scancode::Left => {
                        player.rotate_left(delta);
                    },
                    Scancode::Right => {
                        player.rotate_right(delta);
                    },
                    _ => {}
                }
            }
        }

        if this_frame_time.duration_since(last_asteroid_spawn_time).as_secs_f32() > ASTEROID_SPAWN_TIME {
            asteroids.push(spawn_asteroid());
            last_asteroid_spawn_time = this_frame_time;
        }

        player.update(delta);

        queued_bullet_deletion_indices.clear();
        queued_asteroid_deletion_indices.clear();

        // update bullet positions, check bounds, check collision with asteroids
        for (i, bullet) in bullets.iter_mut().enumerate() {
            bullet.update(delta);
            if bullet.is_out_of_bounds() {
                queued_bullet_deletion_indices.push(i);
            } else {
                'asteroids: for (j, asteroid) in asteroids.iter_mut().enumerate() {
                    match asteroid.was_hit {
                        asteroid::HitState::Hit { .. } => continue 'asteroids,
                        _ => {}
                    }
                    if are_colliding(bullet.get_rect(), asteroid.get_rect()) {
                        score += 1;
                        asteroid.was_hit = asteroid::HitState::Hit { hit_vec: bullet.vel };
                        particles.append(&mut spawn_hit_particles(bullet.pos, 10));
                        queued_bullet_deletion_indices.push(i);
                        queued_asteroid_deletion_indices.push(j);
                    }
                }
            }
        }

        for asteroid in &mut asteroids {
            asteroid.update(delta);
            if player.is_alive {
                if are_colliding(asteroid.get_rect(), player.get_rect()) {
                    player.is_alive = false;
                    play_sound(Sound::Explode);
                    particles.append(&mut player.spawn_death_particles(100));
                }
            }
        }

        for particle in &mut particles {
            particle.update(delta);
        }

        for index in queued_bullet_deletion_indices.iter().rev() {
            bullets.swap_remove(*index);
        }
        for index in queued_asteroid_deletion_indices.iter().rev() {
            asteroids.append(&mut asteroids[*index].get_splits());
            asteroids.swap_remove(*index);
        }

        for particle in &particles {
            particle.draw(&mut canvas)?;
        }
        for asteroid in &asteroids {
            asteroid.draw(&mut canvas)?;
        }
        for bullet in &bullets {
            bullet.draw(&mut canvas)?;
        }
        if player.is_alive {
            player.draw(&mut canvas)?;
        }

        let score_surface = font
            .render(&format!("{}", score))
            .blended(Color::RGBA(50, 255, 50, 200))
            .map_err(|e| e.to_string())?;
        let score_surface_size = score_surface.size();
        let score_texture = texture_creator
            .create_texture_from_surface(&score_surface)
            .map_err(|e| e.to_string())?;
        let x_padding = 20;
        let score_render_rect = rect!(
            SCREEN_WIDTH - score_surface_size.0 - x_padding, 0,
            score_surface_size.0, score_surface_size.1
        );
        canvas.copy(&score_texture, None, score_render_rect)?;

        canvas.present();
    }

    sdl2::mixer::close_audio();

    Ok(())
}

struct Player {
    pos: Vec2,
    vel: Vec2,
    rot: f32,
    is_alive: bool,
}

impl Player {
    const MAX_VELOCITY: f32 = 350.0;
    const ACC_RATE: f32 = 500.0;
    const ROTATION_RATE: f32 = 6.0;
    const BULLET_SPAWN_Y_OFFSET: f32 = -10.0;
    const RADIUS: u32 = 14;

    fn new(x_pos: f32, y_pos: f32) -> Player {
        Player {
            pos: glm::vec2(x_pos, y_pos),
            vel: glm::vec2(0.0, 0.0),
            rot: 0.0,
            is_alive: true,
        }
    }

    fn rotate_left(&mut self, delta: f32) {
        self.rot -= Self::ROTATION_RATE * delta;
    }

    fn rotate_right(&mut self, delta: f32) {
        self.rot += Self::ROTATION_RATE * delta;
    }

    fn accelerate(&mut self, delta: f32) {
        let dir = unit_vec_rotated(self.rot);
        let acc_vec = dir * Self::ACC_RATE;
        self.vel += acc_vec * delta;
        if self.vel.magnitude() > Self::MAX_VELOCITY {
            self.vel = self.vel.normalize() * Self::MAX_VELOCITY;
        }
    }

    fn update(&mut self, delta: f32) {
        self.pos += self.vel * delta;
        self.pos = try_wrap_around_screen(self.pos, Self::RADIUS);
    }

    fn draw(&self, canvas: &mut Canvas<Window>) -> Result<(), String> {
        let left_coord = -(Self::RADIUS as f32) + 2.0;
        let right_coord = -left_coord;
        let top_coord = -(Self::RADIUS as f32) - 4.0;
        let bottom_coord = -top_coord;
        let point_offsets = vec![
            glm::rotate_vec2(&glm::vec2(0.0, top_coord), self.rot),
            glm::rotate_vec2(&glm::vec2(left_coord, bottom_coord), self.rot),
            glm::rotate_vec2(&glm::vec2(right_coord, bottom_coord), self.rot),
        ];
        canvas.set_draw_color(Color::RGB(0, 255, 50));
        for i in 0..point_offsets.len() {
            let curr_point_offset = point_offsets[i];
            let next_point_offset_index = (i + 1) % point_offsets.len();
            let next_point_offset = point_offsets[next_point_offset_index];
            let p1 = self.pos + curr_point_offset;
            let p2 = self.pos + next_point_offset;
            canvas.draw_line((p1.x as i32, p1.y as i32),
                            (p2.x as i32, p2.y as i32))?
        }

        draw_debug_rect(self.get_rect(), canvas)?;

        Ok(())
    }

    fn spawn_death_particles(&self, num: u32) -> Vec<Particle> {
        let mut rng = rand::thread_rng();
        let mut particles = Vec::new();
        let pos = self.pos;
        let min_velocity = 100.0;
        let max_velocity = 200.0;
        for _ in 0..num {
            let angle = rng.gen::<f32>() * 2.0 * std::f32::consts::PI;
            let vel_scalar = rng.gen_range(min_velocity, max_velocity);
            let vel = unit_vec_rotated(angle) * vel_scalar;
            let r = rng.gen();
            let g = rng.gen();
            let b = rng.gen();
            let color = Color::RGB(r, g, b);
            particles.push(Particle::new(pos, vel, color));
        }

        particles
    }

    fn spawn_bullet(&self) -> Bullet {
        let unrotated_pos_offset = glm::vec2(0.0, Self::BULLET_SPAWN_Y_OFFSET);
        let pos_offset = glm::rotate_vec2(&unrotated_pos_offset, self.rot);
        Bullet::new(self.pos + pos_offset, self.rot)
    }

    fn get_rect(&self) -> Rect {
        rect_from_center!(self.pos, Self::RADIUS * 2, Self::RADIUS * 2)
    }
}

struct Bullet {
    pos: Vec2,
    vel: Vec2,
}

impl Bullet {
    const RADIUS: u32 = 3;
    const VELOCITY: f32 = 800.0;

    fn new(pos: Vec2, rot: f32) -> Bullet {
        let vel = unit_vec_rotated(rot) * Self::VELOCITY;
        Bullet {
            pos,
            vel,
        }
    }

    fn is_out_of_bounds(&self) -> bool {
        self.pos.x < 0.0 - Self::RADIUS as f32 ||
            self.pos.y < 0.0 - Self::RADIUS as f32 ||
            self.pos.x > SCREEN_WIDTH as f32 + Self::RADIUS as f32 ||
            self.pos.y > SCREEN_HEIGHT as f32 + Self::RADIUS as f32
    }

    fn update(&mut self, delta: f32) {
        self.pos += self.vel * delta;
    }


    fn draw(&self, canvas: &mut Canvas<Window>) -> Result<(), String> {
        canvas.set_draw_color(Color::WHITE);
        canvas.fill_rect(self.get_rect())?;

        Ok(())
    }

    fn get_rect(&self) -> Rect {
        rect_from_center!(self.pos, Self::RADIUS * 2, Self::RADIUS * 2)
    }
}

struct Asteroid {
    pos: Vec2,
    vel: Vec2,
    radius: u32,
    was_hit: asteroid::HitState,
}

impl Asteroid {
    const MIN_RADIUS: u32 = 10;

    fn new(pos: Vec2, vel: Vec2, radius: u32) -> Asteroid {
        Asteroid {
            pos,
            vel,
            radius,
            was_hit: asteroid::HitState::NotHit,
        }
    }

    fn update(&mut self, delta: f32) {
        self.pos += self.vel * delta;
        self.pos = try_wrap_around_screen(self.pos, self.radius);
    }

    fn draw(&self, canvas: &mut Canvas<Window>) -> Result<(), String> {
        canvas.set_draw_color(Color::GRAY);
        canvas.fill_rect(self.get_rect())?;

        Ok(())
    }

    fn get_splits(&self) -> Vec<Asteroid> {
        let shot_dir = match self.was_hit {
            asteroid::HitState::Hit { hit_vec } => glm::normalize(&hit_vec),
            _ => panic!("Asteroid attemped to split without hit message"),
        };
        let right_dir = glm::rotate_vec2(&-shot_dir, 0.5 * std::f32::consts::PI);
        let left_dir = glm::rotate_vec2(&-shot_dir, -0.5 * std::f32::consts::PI);
        // let rotation_to_shot = glm::angle(&(-shot_dir), &self.vel);
        let new_radius = self.radius / 2;
        if new_radius < Self::MIN_RADIUS {
            return vec![]
        }
        // TODO clamp new velocities to the maximum
        let new_vel_right = glm::magnitude(&self.vel) * right_dir + self.vel * 0.7;
        let new_vel_left = glm::magnitude(&self.vel) * left_dir + self.vel * 0.7;
        // let new_vel_right = glm::rotate_vec2(&self.vel, 0.5 * std::f32::consts::PI);
        // let new_vel_left = -new_vel_right;
        // let new_vel_right = glm::rotate_vec2(&self.vel, rotation_to_shot + (0.5 * std::f32::consts::PI));
        // let new_vel_left = glm::rotate_vec2(&self.vel, rotation_to_shot - (0.5 * std::f32::consts::PI));
        let new_pos1 = self.pos + glm::normalize(&new_vel_right) * new_radius as f32;
        let new_pos2 = self.pos + glm::normalize(&new_vel_left) * new_radius as f32;

        vec! [
            Asteroid::new(new_pos1, new_vel_right, new_radius),
            Asteroid::new(new_pos2, new_vel_left, new_radius),
        ]
    }

    fn get_rect(&self) -> Rect {
        rect_from_center!(self.pos, self.radius * 2, self.radius * 2)
    }
}

mod asteroid {
    extern crate nalgebra_glm as glm;
    use glm::Vec2;

    pub enum HitState {
        NotHit,
        Hit { hit_vec: Vec2 },
    }
}

struct Particle {
    pos: Vec2,
    vel: Vec2,
    color: Color,
}

impl Particle {
    const RADIUS: u32 = 2;

    fn new(pos: Vec2, vel: Vec2, color: Color) -> Particle {
        Particle { pos, vel, color }
    }

    fn update(&mut self, delta: f32) {
        self.pos += self.vel * delta;
        let mut rng = rand::thread_rng();
        let r = rng.gen();
        let g = rng.gen();
        let b = rng.gen();
        self.color = Color::RGB(r, g, b);
    }

    fn draw (&self, canvas: &mut Canvas<Window>) -> Result<(), String> {
        canvas.set_draw_color(self.color);
        canvas.fill_rect(self.get_rect())?;
        Ok(())
    }

    fn get_rect(&self) -> Rect {
        rect_from_center!(self.pos, Self::RADIUS * 2, Self::RADIUS * 2)
    }
}

fn unit_vec_rotated(rot: f32) -> Vec2 {
    let unit_vec_up = glm::vec2(0.0, -1.0);
    glm::rotate_vec2(&unit_vec_up, rot)
}

fn spawn_asteroid() -> Asteroid {
    let mut rng = rand::thread_rng();

    let min_velocity = 30.0f32;
    let max_velocity = 90.0f32;
    let velocity_magnitude = rng.gen_range(min_velocity, max_velocity);
    let rot = rand::random::<f32>() * 2.0 * std::f32::consts::PI;
    let vel = unit_vec_rotated(rot) * velocity_magnitude;

    let min_radius = 20u32;
    let max_radius = 100u32;
    let radius = rng.gen_range(min_radius, max_radius);

    let pos_x = if vel.x > 0.0 {
        -(radius as f32)
    } else {
        SCREEN_WIDTH as f32 + radius as f32
    };
    let pos_y = if vel.y > 0.0 {
        -(radius as f32)
    } else {
        SCREEN_HEIGHT as f32 + radius as f32
    };
    let pos = glm::vec2(pos_x, pos_y);

    Asteroid::new(pos, vel, radius as u32)
}
fn try_wrap_around_screen(pos: Vec2, radius: u32) -> Vec2 {
    let mut result_x = pos.x;
    let mut result_y = pos.y;

    if pos.x > SCREEN_WIDTH as f32 + radius as f32 {
        result_x = -(radius as f32)
    } else if pos.x < -(radius as f32) {
        result_x = SCREEN_WIDTH as f32 + radius as f32
    };
    if pos.y > SCREEN_HEIGHT as f32 + radius as f32 {
        result_y = -(radius as f32)
    } else if pos.y < -(radius as f32) {
        result_y = SCREEN_HEIGHT as f32 + radius as f32
    };

    glm::vec2(result_x, result_y)
}

struct GameState {
    player: Player,
    bullets: Vec<Bullet>,
    asteroids: Vec<Asteroid>,
    particles: Vec<Particle>,
}

#[derive(Debug, EnumString, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
enum Sound {
    Shoot,
    Explode,
    Hit,
}

fn load_all_sounds() -> Result<Vec<sdl2::mixer::Chunk>, String> {
    let mut sounds = Vec::new();
    for variant in Sound::VARIANTS {
        let path = &format!("sounds/{}.wav", variant);
        let file = &Asset::get(path).ok_or(format!("Failed to get file: {}.wav", variant))?;
        sounds.push(load_sound(file)?);
    }

    Ok(sounds)
}

fn load_sound(file: &[u8]) -> Result<sdl2::mixer::Chunk, String> {
    RWops::from_bytes(file)?.load_wav()
}


fn play_sound(sound: Sound) {
    SOUNDS.with(|sounds| {
        let chunk = &sounds[sound as usize];
        match sdl2::mixer::Channel::all().play(chunk, 0) {
            Err(e) if e == "No free channels available" => {
                // TODO in case getting these two values is some kind of a
                // race condition, lets store total allocated channels
                // somewhere else (global game state?)
                let playing_channels = sdl2::mixer::get_playing_channels_number();
                let paused_channels = sdl2::mixer::get_paused_channels_number();
                println!("Not enough channels. Adding another.");
                sdl2::mixer::allocate_channels(playing_channels + paused_channels + 1);
            },
            Err(e) => println!("{}", e),
            Ok(_channel) => {},
        }
    })
}

fn are_colliding(rect1: Rect, rect2: Rect) -> bool {
    rect1.has_intersection(rect2)
}

fn spawn_hit_particles(pos: Vec2, num: u32) -> Vec<Particle> {
    let mut rng = rand::thread_rng();
    let mut particles = Vec::new();
    let min_velocity = 100.0;
    let max_velocity = 200.0;
    for _ in 0..num {
        let angle = rng.gen::<f32>() * 2.0 * std::f32::consts::PI;
        let vel_scalar = rng.gen_range(min_velocity, max_velocity);
        let vel = unit_vec_rotated(angle) * vel_scalar;
        // let r = rng.gen();
        // let g = rng.gen();
        // let b = rng.gen();
        let color = Color::RGB(180, 50, 50);
        particles.push(Particle::new(pos, vel, color));
    }

    particles
}

fn draw_debug_rect(rect: Rect, canvas: &mut Canvas<Window>) -> Result<(), String> {
    let color = Color::RGBA(100, 100, 200, 140);
    canvas.set_draw_color(color);
    canvas.fill_rect(rect)?;

    Ok(())
}

