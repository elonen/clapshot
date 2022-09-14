import { writable } from 'svelte/store';

export const cur_username = writable("Testi Kalanen");
export const cur_user_pic = writable("https://mdbootstrap.com/img/new/avatars/4.jpg");

export let video_is_ready = writable(false);

export let all_comments = writable([]);
