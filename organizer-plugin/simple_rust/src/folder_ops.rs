/*
use crate::graph_interface::GraphInterface;
use crate::models::{PropEdge, PropNode};
use diesel::result::Error;


impl GraphInterface {
    // ...

    pub fn get_node_by_id(&self, node_id: i32) -> Result<PropNode, diesel::result::Error> {
        // TODO: Implement the get_node_by_id function
    }

    pub fn get_nodes_by_type(&self, node_type: &str) -> Result<Vec<PropNode>, diesel::result::Error> {
        // TODO: Implement the get_nodes_by_type function
    }

    pub fn update_node_body(&self, node_id: i32, new_body: &str) -> Result<usize, diesel::result::Error> {
        // TODO: Implement the update_node_body function
    }

    pub fn delete_node(&self, node_id: i32) -> Result<usize, diesel::result::Error> {
        // TODO: Implement the delete_node function
    }

    pub fn delete_edge(&self, edge_id: i32) -> Result<usize, diesel::result::Error> {
        // TODO: Implement the delete_edge function
    }

    pub fn get_edges_by_type(&self, edge_type: &str) -> Result<Vec<PropEdge>, diesel::result::Error> {
        // TODO: Implement the get_edges_by_type function
    }

    pub fn get_edges_by_from_node_id_and_type(&self, from_node_id: i32, edge_type: &str) -> Result<Vec<(PropEdge, PropNode)>, diesel::result::Error> {
        // TODO: Implement the get_edges_by_from_node_id_and_type function
    }

    pub fn get_edges_by_to_node_id_and_type(&self, to_node_id: i32, edge_type: &str) -> Result<Vec<(PropEdge, PropNode)>, diesel::result::Error> {
        // TODO: Implement the get_edges_by_to_node_id_and_type function
    }

    // Add more high-level functions as needed
}


// ------------------------------------------


pub fn create_folder(interface: &GraphInterface, label: &str, parent_id: i32) -> Result<i32, Error> {
    let parent = interface.get_node_by_id(parent_id)?;
    if parent.node_type != "folder" {
        return Err(Error::RollbackTransaction); // or another custom error
    }
    let new_folder_id = interface.create_node("folder", Some(&format!(r#"{{"label": "{}"}}"#, label)))?;
    interface.create_edge(None, None, Some(new_folder_id), None, None, Some(parent_id), "parent_folder", None, 0.0, 0)?;
    Ok(new_folder_id)
}

pub fn move_video_to_folder(interface: &GraphInterface, video_hash: &str, folder_id: i32) -> Result<(), Error> {
    let folder = interface.get_node_by_id(folder_id)?;
    if folder.node_type != "folder" {
        return Err(Error::RollbackTransaction); // or another custom error
    }
    interface.create_edge(Some(video_hash), None, None, None, None, Some(folder_id), "parent_folder", None, 0.0, 0)?;
    Ok(())
}

pub fn move_folder_to_folder(interface: &GraphInterface, folder_id: i32, parent_folder_id: i32) -> Result<(), Error> {
    let folder = interface.get_node_by_id(folder_id)?;
    let parent_folder = interface.get_node_by_id(parent_folder_id)?;

    if folder.node_type != "folder" || parent_folder.node_type != "folder" {
        return Err(Error::RollbackTransaction); // or another custom error
    }
    interface.create_edge(None, None, Some(folder_id), None, None, Some(parent_folder_id), "parent_folder", None, 0.0, 0)?;
    Ok(())
}

pub fn get_folder_videos(interface: &GraphInterface, folder_id: i32) -> Result<Vec<PropEdge>, Error> {
    let folder = interface.get_node_by_id(folder_id)?;
    if folder.node_type != "folder" {
        return Err(Error::RollbackTransaction); // or another custom error
    }
    let videos = interface.get_edges_by_to_node_id_and_type(folder_id, "parent_folder")?;
    Ok(videos.into_iter().map(|(edge, _)| edge).collect())
}

pub fn get_subfolders(interface: &GraphInterface, folder_id: i32) -> Result<Vec<PropEdge>, Error> {
    let folder = interface.get_node_by_id(folder_id)?;
    if folder.node_type != "folder" {
        return Err(Error::RollbackTransaction); // or another custom error
    }
    let subfolders = interface.get_edges_by_from_node_id_and_type(folder_id, "parent_folder")?;
    Ok(subfolders.into_iter().map(|(edge, _)| edge).collect())
}

pub fn create_project(interface: &GraphInterface, label: &str) -> Result<i32, Error> {
    let new_project_id = interface.create_node("folder", Some(&format!(r#"{{"label": "{}"}}"#, label)))?;
    Ok(new_project_id)
}

pub fn add_project_member(interface: &GraphInterface, project_id: i32, username: &str) -> Result<(), Error> {
    let project = interface.get_node_by_id(project_id)?;
    if project.node_type != "folder" {
        return Err(Error::RollbackTransaction); // or another custom error
    }
    interface.add_project_member(project_id, username)?;
    Ok(())
}

pub fn remove_project_member(interface: &GraphInterface, project_id: i32, username: &str) -> Result<(), Error> {
    let project = interface.get_node_by_id(project_id)?;
    if project.node_type != "folder" {
        return Err(Error::RollbackTransaction); // or another custom error
    }
    interface.remove_project_member(project_id, username)?;
    Ok(())
}

pub fn project_members(interface: &GraphInterface, project_id: i32) -> Result<Vec<(PropEdge, PropNode)>, Error> {
    let project = interface.get_node_by_id(project_id)?;
    if project.node_type != "folder" {
        return Err(Error::RollbackTransaction); // or another custom error
    }
    let members = interface.project_members(project_id)?;
    Ok(members)
}
*/
