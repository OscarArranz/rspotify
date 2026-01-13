use std::time::Duration;

use gpui::{
    Animation, AnimationExt, Context, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Window, div, ease_in_out, img, px, rgb,
};
use rspotify::prelude::OAuthClient;
use tokio::runtime::Runtime;

use crate::hooks::use_spotify;

const AVATAR_SIZE: f32 = 47.0;
const AVATAR_BG_COLOR: u32 = 0x1f1f1f;
const AVATAR_IMAGE_SIZE: f32 = 32.0;
const AVATAR_ANIMATION_TYPE: &str = "scale";
const AVATAR_ANIMATION_DURATION: u64 = 100;
const AVATAR_ANIMATION_DELTA_FACTOR: f32 = 1.0;

enum UserAvatarState {
    Loading,
    Loaded(SharedString),
    Error,
}

#[derive(Clone, Copy, PartialEq)]
enum HoverState {
    Idle,
    Entering,
    Leaving,
}

pub struct UserAvatar {
    avatar: UserAvatarState,
    hover_state: HoverState,
}

impl UserAvatar {
    pub fn new() -> Self {
        UserAvatar {
            avatar: UserAvatarState::Loading,
            hover_state: HoverState::Idle,
        }
    }

    fn fetch_avatar(&mut self, cx: &mut Context<Self>) {
        if let Some(spotify) = use_spotify(cx) {
            cx.spawn(async move |this, cx| {
                let rt = Runtime::new().unwrap();

                let user_data = rt.block_on(async { spotify.me().await }).unwrap();
                println!("User data: {:?}", user_data);
                let avatar_url = user_data.images.unwrap().first().unwrap().url.clone();

                this.update(cx, |this, cx| {
                    this.avatar = UserAvatarState::Loaded(avatar_url.into());
                    cx.notify();
                })
                .inspect(|_| println!("Avatar loaded"))
                .inspect_err(|e| println!("Error loading avatar: {}", e))
                .ok();
            })
            .detach()
        }
    }
}

impl Render for UserAvatar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main_container = div()
            .h_full()
            .w(px(AVATAR_SIZE + AVATAR_ANIMATION_DELTA_FACTOR * 2.0))
            .flex()
            .items_center()
            .justify_center();

        let avatar_container = div()
            .id("user-avatar")
            .flex()
            .items_center()
            .justify_center()
            .size(px(AVATAR_SIZE))
            .rounded_full()
            .cursor_pointer()
            .overflow_hidden()
            .bg(rgb(AVATAR_BG_COLOR))
            .on_hover(cx.listener(|this, &hovered, _window, cx| {
                this.hover_state = if hovered {
                    HoverState::Entering
                } else {
                    HoverState::Leaving
                };
                cx.notify();
            }));

        match &self.avatar {
            UserAvatarState::Loading => {
                self.fetch_avatar(cx);

                let animated_container = avatar_container.with_animation(
                    AVATAR_ANIMATION_TYPE,
                    Animation::new(Duration::from_millis(AVATAR_ANIMATION_DURATION))
                        .repeat()
                        .with_easing(ease_in_out),
                    |div, delta| {
                        let size = AVATAR_SIZE + (delta * AVATAR_ANIMATION_DELTA_FACTOR);
                        div.size(px(size))
                    },
                );

                main_container.child(animated_container)
            }
            UserAvatarState::Loaded(url) => {
                let hover_state = self.hover_state;

                match hover_state {
                    HoverState::Entering => {
                        // When hovered, apply scale-up animation to both container and image
                        let avatar_image = img(url.clone())
                            .size(px(AVATAR_IMAGE_SIZE))
                            .rounded_full()
                            .with_animation(
                                "image-scale-enter",
                                Animation::new(Duration::from_millis(AVATAR_ANIMATION_DURATION))
                                    .with_easing(ease_in_out),
                                |img_el, delta| {
                                    let size =
                                        AVATAR_IMAGE_SIZE + (delta * AVATAR_ANIMATION_DELTA_FACTOR);
                                    img_el.size(px(size))
                                },
                            );

                        let animated_container =
                            avatar_container.child(avatar_image).with_animation(
                                "container-scale-enter",
                                Animation::new(Duration::from_millis(AVATAR_ANIMATION_DURATION))
                                    .with_easing(ease_in_out),
                                |div, delta| {
                                    let size =
                                        AVATAR_SIZE + (delta * AVATAR_ANIMATION_DELTA_FACTOR);
                                    div.size(px(size))
                                },
                            );

                        main_container.child(animated_container)
                    }
                    HoverState::Leaving => {
                        // When hover lost, apply scale-down animation (reverse direction)
                        let avatar_image = img(url.clone())
                            .size(px(AVATAR_IMAGE_SIZE + AVATAR_ANIMATION_DELTA_FACTOR))
                            .rounded_full()
                            .with_animation(
                                "image-scale-leave",
                                Animation::new(Duration::from_millis(AVATAR_ANIMATION_DURATION))
                                    .with_easing(ease_in_out),
                                |img_el, delta| {
                                    // Shrink from expanded size back to normal
                                    let size = AVATAR_IMAGE_SIZE + AVATAR_ANIMATION_DELTA_FACTOR
                                        - (delta * AVATAR_ANIMATION_DELTA_FACTOR);
                                    img_el.size(px(size))
                                },
                            );

                        let animated_container =
                            avatar_container.child(avatar_image).with_animation(
                                "container-scale-leave",
                                Animation::new(Duration::from_millis(AVATAR_ANIMATION_DURATION))
                                    .with_easing(ease_in_out),
                                |div, delta| {
                                    // Shrink from expanded size back to normal
                                    let size = AVATAR_SIZE + AVATAR_ANIMATION_DELTA_FACTOR
                                        - (delta * AVATAR_ANIMATION_DELTA_FACTOR);
                                    div.size(px(size))
                                },
                            );

                        main_container.child(animated_container)
                    }
                    HoverState::Idle => {
                        // Static state - show normal size avatar
                        let avatar_container_with_image = avatar_container
                            .child(img(url.clone()).size(px(AVATAR_IMAGE_SIZE)).rounded_full());

                        main_container.child(avatar_container_with_image)
                    }
                }
            }
            UserAvatarState::Error => {
                let animated_container = avatar_container.with_animation(
                    AVATAR_ANIMATION_TYPE,
                    Animation::new(Duration::from_millis(AVATAR_ANIMATION_DURATION))
                        .repeat()
                        .with_easing(ease_in_out),
                    |div, delta| {
                        let size = AVATAR_SIZE + (delta * AVATAR_ANIMATION_DELTA_FACTOR);
                        div.size(px(size))
                    },
                );

                main_container.child(animated_container)
            }
        }
    }
}
