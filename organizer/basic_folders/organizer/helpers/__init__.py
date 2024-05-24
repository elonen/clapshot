import clapshot_grpc.proto.clapshot as clap


# Map media type to FontAwesome icon visualization
def media_type_to_vis_icon(media_type: str) -> clap.PageItemFolderListingItemVisualization:
    class_names = {
        "video": "fas fa-video",
        "image": "fas fa-image",
        "audio": "fas fa-volume-high",
    }.get(media_type.lower(), "fas fa-question")
    return clap.PageItemFolderListingItemVisualization(icon=clap.Icon(fa_class=clap.IconFaClass(classes=class_names)))
