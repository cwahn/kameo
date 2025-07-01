use std::time::Duration;

use futures::TryStreamExt;
use kameo::{
    actor::RemoteActorRef,
    prelude::{Context, Message},
    remote::{ActorSwarm, ActorSwarmBehaviour},
    remote_message, Actor, RemoteActor,
};
use libp2p::{
    kad::{self, store::MemoryStore},
    mdns,
    request_response::{self, ProtocolSupport},
    swarm::dial_opts::DialOpts,
    StreamProtocol, SwarmBuilder,
};

use log::{error, info};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

#[derive(Actor, RemoteActor)]
pub struct MyActor {
    count: i64,
}

#[derive(Serialize, Deserialize)]
pub struct Inc {
    amount: u32,
}

#[remote_message("3b9128f1-0593-44a0-b83a-f4188baa05bf")]
impl Message<Inc> for MyActor {
    type Reply = i64;

    async fn handle(&mut self, msg: Inc, _ctx: &mut Context<Self, Self::Reply>) -> Self::Reply {
        println!("incrementing");
        self.count += msg.amount as i64;
        self.count
    }
}

#[derive(Serialize, Deserialize)]
pub struct Dec {
    amount: u32,
}

#[remote_message("20185b42-8645-47d2-8d65-2d1c68d26823")]
impl Message<Dec> for MyActor {
    type Reply = i64;

    async fn handle(&mut self, msg: Dec, _ctx: &mut Context<Self, Self::Reply>) -> Self::Reply {
        println!("decrementing");
        self.count -= msg.amount as i64;
        self.count
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info".parse::<EnvFilter>()?)
        .without_time()
        .with_target(false)
        .init();

    let is_host = match std::env::args().nth(1).as_deref() {
        // Some("guest") => false,
        Some("host") => true,
        // Some(_) | None => {
        //     error!("expected either 'host' or 'guest' argument");
        //     return Ok(());
        // }
        _ => false,
    };

    // Bootstrap the actor swarm
    if is_host {
        // ActorSwarm::bootstrap()?
        //     .listen_on("/ip4/0.0.0.0/udp/8020/quic-v1".parse()?)
        //     .await?;

        let _ = ActorSwarm::bootstrap_with_swarm(
            SwarmBuilder::with_new_identity()
                .with_tokio()
                .with_tcp(
                    Default::default(),
                    // (libp2p_tls::Config::new, libp2p_noise::Config::new),
                    libp2p_noise::Config::new,
                    libp2p_yamux::Config::default,
                )?
                .with_behaviour(|keypair| {
                    Ok(ActorSwarmBehaviour {
                        kademlia: kad::Behaviour::new(
                            keypair.public().to_peer_id(),
                            MemoryStore::new(keypair.public().to_peer_id()),
                        ),
                        mdns: mdns::tokio::Behaviour::new(
                            mdns::Config::default(),
                            keypair.public().to_peer_id(),
                        )?,
                        request_response: request_response::cbor::Behaviour::new(
                            [(StreamProtocol::new("/kameo/1"), ProtocolSupport::Full)],
                            request_response::Config::default(),
                        ),
                    })
                })?
                .build(),
        )?
        // .listen_on(MULTI_ADDR.parse()?)
        .listen_on("/ip4/0.0.0.0/tcp/8020".parse()?)
        // .listen_on("/ip4/0.0.0.0/tcp/8020/noise".parse()?)
        .await?;
    } else {
        let res = ActorSwarm::bootstrap_with_swarm(
            SwarmBuilder::with_new_identity()
                .with_tokio()
                .with_tcp(
                    Default::default(),
                    // (libp2p_tls::Config::new, libp2p_noise::Config::new),
                    libp2p_noise::Config::new,
                    libp2p_yamux::Config::default,
                )?
                .with_behaviour(|keypair| {
                    Ok(ActorSwarmBehaviour {
                        kademlia: kad::Behaviour::new(
                            keypair.public().to_peer_id(),
                            MemoryStore::new(keypair.public().to_peer_id()),
                        ),
                        mdns: mdns::tokio::Behaviour::new(
                            mdns::Config::default(),
                            keypair.public().to_peer_id(),
                        )?,
                        request_response: request_response::cbor::Behaviour::new(
                            [(StreamProtocol::new("/kameo/1"), ProtocolSupport::Full)],
                            request_response::Config::default(),
                        ),
                    })
                })?
                .build(),
        )?
        .dial(
            DialOpts::unknown_peer_id()
                .address("/ip4/127.0.0.1/tcp/8020".parse()?)
                .build(),
        )
        .await;

        match res {
            Ok(peer_id) => info!("connected to host with peer ID: {peer_id}"),
            Err(e) => {
                error!("failed to connect to host: {e}");
            }
        }
    }

    if is_host {
        let actor_ref = MyActor::spawn(MyActor { count: 0 });
        info!("registering actor");
        actor_ref.register("my_actor").await?;
    } else {
        // Wait for registry to sync
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    loop {
        if !is_host {
            let mut remote_actor_refs = RemoteActorRef::<MyActor>::lookup_all("my_actor");
            let mut found = 0;
            while let Some(remote_actor_ref) = remote_actor_refs.try_next().await? {
                let count = remote_actor_ref.ask(&Inc { amount: 10 }).await?;
                println!("Incremented! Count is {count}");
                found += 1;
            }
            if found == 0 {
                println!("actor not found");
            }
        }

        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
