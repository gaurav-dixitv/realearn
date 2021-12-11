use reaper_high::{Project, Reaper, Track};
use reaper_medium::{MediaTrack, TrackAttributeKey};

pub fn get_track_level(mut track: MediaTrack) -> u32 {
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