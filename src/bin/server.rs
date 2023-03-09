use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use message_io::{network::Endpoint, node};

fn main() {
    let (handler, listener) = node::split::<()>();
    dotenvy::dotenv().ok();

    let server_name = dotenvy::var("SERVER_NAME").unwrap();
    let tcp_port = dotenvy::var("TCP_PORT").unwrap();
    let ws_port = dotenvy::var("WS_PORT").unwrap();
    let api_key = dotenvy::var("API_KEY").unwrap();

    // listen tcp and ws
    let (tcp_id, _tcp_sock) = handler
        .network()
        .listen(
            message_io::network::Transport::Tcp,
            format!("{server_name}:{tcp_port}"),
        )
        .unwrap();
    let (ws_id, _ws_sock) = handler
        .network()
        .listen(
            message_io::network::Transport::Ws,
            format!("{server_name}:{ws_port}"),
        )
        .unwrap();

    let mut ws_endpoints: HashSet<Endpoint> = HashSet::new();
    let mut auth_endpoint: Option<Endpoint> = None;

    listener.for_each(move |event| {
        println!("Active WS connections: {:?}", ws_endpoints.len());
        match event.network() {
            message_io::network::NetEvent::Connected(_, _) => println!("Connected"),
            message_io::network::NetEvent::Accepted(e, _) => {
                println!("e_resource_id: {:?}, ws_id: {:?}", e.resource_id(), ws_id);
                if e.resource_id().adapter_id() == ws_id.adapter_id() {
                    // add the new ws connection to the set
                    ws_endpoints.insert(e);
                    println!("New ws connection from {:?}", e.addr());
                } else {
                    println!("New tcp connection from {:?}", e.addr());
                }
            }
            message_io::network::NetEvent::Message(e, d) => {
                // we should only get messages from tcp
                if e.resource_id().adapter_id() != tcp_id.adapter_id() {
                    println!("Unexpected message from {:?}", e.addr());
                    return;
                } else if auth_endpoint.is_none() {
                    println!("No auth endpoint set, checking if this is the auth endpoint");
                    let auth_endpoint_str = String::from_utf8(d.to_vec()).unwrap();
                    if auth_endpoint_str == api_key {
                        println!("Auth endpoint set to {:?}", e.addr());
                        auth_endpoint = Some(e);
                    } else {
                        println!("Invalid auth endpoint, disconnecting");
                    }
                    return;
                }
                println!("Message from {:?}: {:?}", e.addr(), d);

                if auth_endpoint.is_none() || e != auth_endpoint.unwrap() {
                    println!("UNAUTHORIZED TCP CONNECTION, DROPPING!!!!");
                    return;
                }

                // send the message to all ws connections
                for ws_endpoint in ws_endpoints.iter() {
                    handler.network().send(*ws_endpoint, d);
                }
            }
            message_io::network::NetEvent::Disconnected(e) => {
                // remove the ws connection from the set, if it was a ws connection
                ws_endpoints.remove(&e);

                // if the auth endpoint disconnected, clear it
                if auth_endpoint.is_some() && e == auth_endpoint.unwrap() {
                    println!("TCP endpoint disconnected");
                    auth_endpoint = None;
                }

                println!("Disconnected from {:?} ({})", e.addr(), e.resource_id());
            }
        }
    })
}
