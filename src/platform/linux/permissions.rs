//! Linux WebKitGTK 6.0 site permission request mediation

use crate::platform::traits::{PermissionMediator, PlatformPermissionKind};
use webkit6::prelude::*;
use webkit6::{
    ClipboardPermissionRequest, DeviceInfoPermissionRequest, GeolocationPermissionRequest,
    NotificationPermissionRequest, PermissionRequest, UserMediaPermissionRequest,
};

pub struct LinuxPermissionMediator {
    request: PermissionRequest,
}

impl LinuxPermissionMediator {
    pub fn new(request: PermissionRequest) -> Self {
        Self { request }
    }
}

impl PermissionMediator for LinuxPermissionMediator {
    fn identify_request(&self) -> (PlatformPermissionKind, String) {
        if self.request.is::<GeolocationPermissionRequest>() {
            (
                PlatformPermissionKind::Geolocation,
                "wants to know your physical location".to_string(),
            )
        } else if self.request.is::<NotificationPermissionRequest>() {
            (
                PlatformPermissionKind::Notifications,
                "wants to display desktop notifications".to_string(),
            )
        } else if let Some(media_req) = self.request.downcast_ref::<UserMediaPermissionRequest>() {
            let is_audio = webkit6::functions::user_media_permission_is_for_audio_device(media_req);
            let is_video = webkit6::functions::user_media_permission_is_for_video_device(media_req);

            if is_audio && is_video {
                (
                    PlatformPermissionKind::CameraAndMicrophone,
                    "wants to use your camera and microphone".to_string(),
                )
            } else if is_video {
                (
                    PlatformPermissionKind::Camera,
                    "wants to use your camera".to_string(),
                )
            } else if is_audio {
                (
                    PlatformPermissionKind::Microphone,
                    "wants to use your microphone".to_string(),
                )
            } else {
                (
                    PlatformPermissionKind::Other,
                    "wants to access media devices".to_string(),
                )
            }
        } else if self.request.is::<ClipboardPermissionRequest>() {
            (
                PlatformPermissionKind::Clipboard,
                "wants to access your clipboard".to_string(),
            )
        } else if self.request.is::<DeviceInfoPermissionRequest>() {
            (
                PlatformPermissionKind::DeviceInfo,
                "wants access to audio/video device info".to_string(),
            )
        } else {
            (
                PlatformPermissionKind::Other,
                "wants access to hardware resources".to_string(),
            )
        }
    }

    fn allow(&self) {
        self.request.allow();
    }

    fn deny(&self) {
        self.request.deny();
    }
}
