use std::{pin::Pin, sync::Arc};

use crate::Socket;

type BoxedFuture<O> = Pin<Box<dyn std::future::Future<Output = crate::Result<O>> + Send + 'static>>;

pub struct ProviderWrapper<A, O> {
    provider: Arc<Box<dyn Provider<A, Output = BoxedFuture<O>> + Send + Sync + 'static>>,
}

pub struct ProviderTransfer<A> {
    provider: Arc<Box<dyn Provider<A, Output = BoxedFuture<A>> + Send + Sync + 'static>>,
}

pub struct ProviderChain<A> {
    self_provider: ProviderTransfer<A>,
    other_provider: ProviderTransfer<A>,
}

pub trait Provider<C> {
    type Output;

    fn call(&self, arg: C) -> Self::Output;
}

pub struct ClientProvider<C> {
    pub server_socket: Socket,
    pub connect_provider: Arc<C>,
}

pub struct ServerProvider<S, C> {
    pub accepter_provider: Arc<S>,
    pub connector_provider: Arc<C>,
}

impl<SF, CF, S, O> ServerProvider<SF, CF>
where
    SF: Provider<Socket, Output = BoxedFuture<S>> + 'static,
    CF: Provider<Socket, Output = BoxedFuture<O>> + 'static,
    S: 'static,
    O: 'static,
{
    #[inline]
    pub async fn bind<Sock: Into<Socket>>(&self, socket: Sock) -> crate::Result<S> {
        let socket = socket.into();
        self.accepter_provider.call(socket).await
    }

    #[inline]
    pub async fn connect<Sock: Into<Socket>>(&self, socket: Sock) -> crate::Result<O> {
        let socket = socket.into();
        self.connector_provider.call(socket).await
    }
}

impl<S, C> Clone for ServerProvider<S, C> {
    fn clone(&self) -> Self {
        Self {
            accepter_provider: self.accepter_provider.clone(),
            connector_provider: self.connector_provider.clone(),
        }
    }
}

impl<C, O> ClientProvider<C>
where
    C: Provider<Socket, Output = BoxedFuture<O>>,
    O: Send + 'static,
{
    pub(crate) fn set_server_socket(mut self, socket: Socket) -> Self {
        self.server_socket = socket;
        self
    }

    pub(crate) fn default_socket(&self) -> &Socket {
        &self.server_socket
    }

    pub async fn connect<A: Into<Socket>>(&self, socket: A) -> crate::Result<O> {
        self.connect_provider.call(socket.into()).await
    }
}

impl<C, O> Provider<Socket> for ClientProvider<C>
where
    C: Provider<Socket, Output = BoxedFuture<O>>,
    O: Send + 'static,
{
    type Output = C::Output;

    fn call(&self, arg: Socket) -> Self::Output {
        self.connect_provider.call(arg)
    }
}

impl<C> Clone for ClientProvider<C> {
    fn clone(&self) -> Self {
        Self {
            server_socket: self.server_socket.clone(),
            connect_provider: self.connect_provider.clone(),
        }
    }
}

impl<A, O> ProviderWrapper<A, O> {
    pub fn wrap<F>(provider: F) -> Self
    where
        F: Provider<A, Output = BoxedFuture<O>> + Send + Sync + 'static,
    {
        Self {
            provider: Arc::new(Box::new(provider)),
        }
    }
}

impl<A> ProviderTransfer<A> {
    pub fn wrap<F>(provider: F) -> Self
    where
        F: Provider<A, Output = BoxedFuture<A>> + Send + Sync + 'static,
    {
        Self {
            provider: Arc::new(Box::new(provider)),
        }
    }
}

impl<A> ProviderChain<A> {
    pub fn chain<F1, F2>(provider: F1, next: F2) -> Self
    where
        F1: Provider<A, Output = BoxedFuture<A>> + Send + Sync + 'static,
        F2: Provider<A, Output = BoxedFuture<A>> + Send + Sync + 'static,
    {
        Self {
            self_provider: ProviderTransfer::wrap(provider),
            other_provider: ProviderTransfer::wrap(next),
        }
    }
}

impl<A, O> Provider<A> for ProviderWrapper<A, O>
where
    A: Send + 'static,
    O: Send + 'static,
{
    type Output = BoxedFuture<O>;

    fn call(&self, cfg: A) -> Self::Output {
        self.provider.call(cfg)
    }
}

impl<A> Provider<A> for ProviderTransfer<A> {
    type Output = BoxedFuture<A>;

    fn call(&self, cfg: A) -> Self::Output {
        self.provider.call(cfg)
    }
}

impl<A> Provider<A> for ProviderChain<A>
where
    A: Send + 'static,
{
    type Output = BoxedFuture<A>;

    fn call(&self, cfg: A) -> Self::Output {
        let this = self.clone();
        Box::pin(async move { this.other_provider.call(this.self_provider.call(cfg).await?).await })
    }
}

impl<A, O> Clone for ProviderWrapper<A, O> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            provider: self.provider.clone(),
        }
    }
}

impl<A> Clone for ProviderTransfer<A> {
    fn clone(&self) -> Self {
        Self {
            provider: self.provider.clone(),
        }
    }
}

impl<A> Clone for ProviderChain<A> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            self_provider: self.self_provider.clone(),
            other_provider: self.other_provider.clone(),
        }
    }
}
