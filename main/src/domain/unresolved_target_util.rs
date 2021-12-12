use reaper_high::{Project, Reaper, Track, FxChain, Fx};
use reaper_medium::{MediaTrack, TrackAttributeKey};
use wildmatch::WildMatch;

fn get_track_level(mut track: MediaTrack) -> u32 {
    let mut level: u32 = 0;
    let reaper = Reaper::get().medium_reaper();

    let mut found = false;
    while found != true {

        let raw_track = unsafe {
            reaper.get_set_media_track_info_get_par_track(track)
        };
        match raw_track {
            None => {
                found = true;
                return level;
            },
            Some(raw_track) => {
                level = level + 1;
                track = raw_track;
            }
        }
    }
    return level;
}


pub fn get_level_indices(project: &Project, level: u32) -> Vec<u32> {
    let mut vec = Vec::new();
    let reaper = Reaper::get().medium_reaper();
    
    let mut track_index = 0;
    while track_index < reaper.count_tracks(project.context()){
        
        let raw_track = reaper.get_track( project.context(), track_index,);
        match raw_track {
            None => (),
            Some(raw_track) => {
                let raw_level = get_track_level(raw_track);
                if raw_level == level {
                    vec.push(track_index)
                }
            }
        }

        track_index = track_index + 1;
    }
    return vec;
}


pub fn get_folder_track_indices(project: &Project) -> Vec<u32> {
    
    let mut vec = Vec::new();
    let reaper = Reaper::get().medium_reaper();
    
    let mut track_index = 0;
    while track_index < reaper.count_tracks(project.context()){
        
        let raw_track = reaper.get_track( project.context(), track_index,);
        match raw_track {
            None => (),
            Some(raw_track) => {
                let is_parent = unsafe { 
                    reaper.get_media_track_info_value(raw_track, TrackAttributeKey::FolderDepth) as i32
                };
                if is_parent == 1 || is_parent == 0 {
                    vec.push(track_index)
                }
            }
        }

        track_index = track_index + 1;
    }

    return vec;
}

fn find_fxs_by_name<'a>(chain: &'a FxChain, name: &'a WildMatch) -> impl Iterator<Item = Fx> + 'a {
    chain
        .fxs()
        .filter(move |fx| name.matches(fx.name().to_str()))
}

pub fn get_track_at_index_with_fx(project: &Project, name: &str, index: u32) -> Option<f64> {

    let reaper = Reaper::get().medium_reaper();    
    let mut track_index = 0;
    let tracks = project.tracks();

    let mut count:i32 = -1;
    for track in tracks {
        let chain = track.normal_fx_chain();
        let mut found = find_fxs_by_name(&chain, &WildMatch::new(format!("*{}*", name).as_str())).next();
        if found.is_some() {
            count = count + 1;
            if count >= index as i32 {
                let raw_index = track.index();
                match raw_index {
                    None => (),
                    Some(raw_index) => {
                        return Some(raw_index as f64);
                    }
                }
            }
        }
    }

    None
}
