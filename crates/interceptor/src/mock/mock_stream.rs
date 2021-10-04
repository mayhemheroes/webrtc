use crate::error::Error;
use crate::stream_info::StreamInfo;
use crate::{Attributes, Interceptor, RTCPReader, RTCPWriter, RTPReader, RTPWriter};

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use util::{Marshal, Unmarshal};

/// MockStream is a helper struct for testing interceptors.
pub struct MockStream {
    interceptor: Arc<dyn Interceptor + Send + Sync>,

    rtcp_writer: Mutex<Option<Arc<dyn RTCPWriter + Send + Sync>>>,
    rtp_writer: Mutex<Option<Arc<dyn RTPWriter + Send + Sync>>>,

    rtcp_out_modified_tx: mpsc::Sender<Box<dyn rtcp::packet::Packet + Send + Sync>>,
    rtp_out_modified_tx: mpsc::Sender<rtp::packet::Packet>,
    rtcp_in_rx: Mutex<mpsc::Receiver<Box<dyn rtcp::packet::Packet + Send + Sync>>>,
    rtp_in_rx: Mutex<mpsc::Receiver<rtp::packet::Packet>>,

    rtcp_out_modified_rx: Mutex<mpsc::Receiver<Box<dyn rtcp::packet::Packet + Send + Sync>>>,
    rtp_out_modified_rx: Mutex<mpsc::Receiver<rtp::packet::Packet>>,
    rtcp_in_tx: Mutex<Option<mpsc::Sender<Box<dyn rtcp::packet::Packet + Send + Sync>>>>,
    rtp_in_tx: Mutex<Option<mpsc::Sender<rtp::packet::Packet>>>,

    rtcp_in_modified_rx: Mutex<mpsc::Receiver<RTCPWithError>>,
    rtp_in_modified_rx: Mutex<mpsc::Receiver<RTPWithError>>,
}

/// RTPWithError is used to send an rtp packet or an error on a channel
pub enum RTPWithError {
    Pkt(rtp::packet::Packet),
    Err(anyhow::Error),
}

/// RTCPWithError is used to send a batch of rtcp packets or an error on a channel
pub enum RTCPWithError {
    Pkt(Box<dyn rtcp::packet::Packet + Send + Sync>),
    Err(anyhow::Error),
}

impl MockStream {
    /// new creates a new MockStream
    pub async fn new(
        info: &StreamInfo,
        interceptor: Arc<dyn Interceptor + Send + Sync>,
    ) -> Arc<Self> {
        let (rtcp_in_tx, rtcp_in_rx) = mpsc::channel(1000);
        let (rtp_in_tx, rtp_in_rx) = mpsc::channel(1000);
        let (rtcp_out_modified_tx, rtcp_out_modified_rx) = mpsc::channel(1000);
        let (rtp_out_modified_tx, rtp_out_modified_rx) = mpsc::channel(1000);
        let (rtcp_in_modified_tx, rtcp_in_modified_rx) = mpsc::channel(1000);
        let (rtp_in_modified_tx, rtp_in_modified_rx) = mpsc::channel(1000);

        let stream = Arc::new(MockStream {
            interceptor: Arc::clone(&interceptor),

            rtcp_writer: Mutex::new(None),
            rtp_writer: Mutex::new(None),

            rtcp_in_tx: Mutex::new(Some(rtcp_in_tx)),
            rtp_in_tx: Mutex::new(Some(rtp_in_tx)),
            rtcp_in_rx: Mutex::new(rtcp_in_rx),
            rtp_in_rx: Mutex::new(rtp_in_rx),

            rtcp_out_modified_tx,
            rtp_out_modified_tx,
            rtcp_out_modified_rx: Mutex::new(rtcp_out_modified_rx),
            rtp_out_modified_rx: Mutex::new(rtp_out_modified_rx),

            rtcp_in_modified_rx: Mutex::new(rtcp_in_modified_rx),
            rtp_in_modified_rx: Mutex::new(rtp_in_modified_rx),
        });

        let rtcp_writer = interceptor
            .bind_rtcp_writer(Arc::clone(&stream) as Arc<dyn RTCPWriter + Send + Sync>)
            .await;
        {
            let mut rw = stream.rtcp_writer.lock().await;
            *rw = Some(rtcp_writer);
        }
        let rtp_writer = interceptor
            .bind_local_stream(
                info,
                Arc::clone(&stream) as Arc<dyn RTPWriter + Send + Sync>,
            )
            .await;
        {
            let mut rw = stream.rtp_writer.lock().await;
            *rw = Some(rtp_writer);
        }

        let rtcp_reader = interceptor
            .bind_rtcp_reader(Arc::clone(&stream) as Arc<dyn RTCPReader + Send + Sync>)
            .await;
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1500];
            let a = Attributes::new();
            loop {
                let n = match rtcp_reader.read(&mut buf, &a).await {
                    Ok((n, _)) => n,
                    Err(err) => {
                        if !Error::ErrIoEOF.equal(&err) {
                            let _ = rtcp_in_modified_tx.send(RTCPWithError::Err(err)).await;
                        }
                        break;
                    }
                };

                let mut b = &buf[..n];
                let pkt = match rtcp::packet::unmarshal(&mut b) {
                    Ok(pkt) => pkt,
                    Err(err) => {
                        let _ = rtcp_in_modified_tx.send(RTCPWithError::Err(err)).await;
                        break;
                    }
                };

                let _ = rtcp_in_modified_tx.send(RTCPWithError::Pkt(pkt)).await;
            }
        });

        let rtp_reader = interceptor
            .bind_remote_stream(
                info,
                Arc::clone(&stream) as Arc<dyn RTPReader + Send + Sync>,
            )
            .await;
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1500];
            let a = Attributes::new();
            loop {
                let n = match rtp_reader.read(&mut buf, &a).await {
                    Ok((n, _)) => n,
                    Err(err) => {
                        if !Error::ErrIoEOF.equal(&err) {
                            let _ = rtp_in_modified_tx.send(RTPWithError::Err(err)).await;
                        }
                        break;
                    }
                };

                let mut b = &buf[..n];
                let pkt = match rtp::packet::Packet::unmarshal(&mut b) {
                    Ok(pkt) => pkt,
                    Err(err) => {
                        let _ = rtp_in_modified_tx.send(RTPWithError::Err(err)).await;
                        break;
                    }
                };

                let _ = rtp_in_modified_tx.send(RTPWithError::Pkt(pkt)).await;
            }
        });

        stream
    }

    /// write_rtcp writes a batch of rtcp packet to the stream, using the interceptor
    pub async fn write_rtcp(
        &self,
        pkt: &(dyn rtcp::packet::Packet + Send + Sync),
    ) -> Result<usize> {
        let a = Attributes::new();
        let rtcp_writer = self.rtcp_writer.lock().await;
        if let Some(writer) = &*rtcp_writer {
            writer.write(pkt, &a).await
        } else {
            Err(Error::new("invalid rtcp_writer".to_owned()).into())
        }
    }

    /// write_rtp writes an rtp packet to the stream, using the interceptor
    pub async fn write_rtp(&self, pkt: &rtp::packet::Packet) -> Result<usize> {
        let a = Attributes::new();
        let rtp_writer = self.rtp_writer.lock().await;
        if let Some(writer) = &*rtp_writer {
            writer.write(pkt, &a).await
        } else {
            Err(Error::new("invalid rtp_writer".to_owned()).into())
        }
    }

    /// receive_rtcp schedules a new rtcp batch, so it can be read be the stream
    pub async fn receive_rtcp(&self, pkt: Box<dyn rtcp::packet::Packet + Send + Sync>) {
        let rtcp_in_tx = self.rtcp_in_tx.lock().await;
        if let Some(tx) = &*rtcp_in_tx {
            let _ = tx.try_send(pkt);
        }
    }

    /// receive_rtp schedules a rtp packet, so it can be read be the stream
    pub async fn receive_rtp(&self, pkt: rtp::packet::Packet) {
        let rtp_in_tx = self.rtp_in_tx.lock().await;
        if let Some(tx) = &*rtp_in_tx {
            let _ = tx.try_send(pkt);
        }
    }

    /// written_rtcp returns a channel containing the rtcp batches written, modified by the interceptor
    pub async fn written_rtcp(&self) -> Option<Box<dyn rtcp::packet::Packet + Send + Sync>> {
        let mut rtcp_out_modified_rx = self.rtcp_out_modified_rx.lock().await;
        rtcp_out_modified_rx.recv().await
    }

    /// written_rtp returns a channel containing rtp packets written, modified by the interceptor
    pub async fn written_rtp(&self) -> Option<rtp::packet::Packet> {
        let mut rtp_out_modified_rx = self.rtp_out_modified_rx.lock().await;
        rtp_out_modified_rx.recv().await
    }

    /// read_rtcp returns a channel containing the rtcp batched read, modified by the interceptor
    pub async fn read_rtcp(&self) -> Option<RTCPWithError> {
        let mut rtcp_in_modified_rx = self.rtcp_in_modified_rx.lock().await;
        rtcp_in_modified_rx.recv().await
    }

    /// read_rtp returns a channel containing the rtp packets read, modified by the interceptor
    pub async fn read_rtp(&self) -> Option<RTPWithError> {
        let mut rtp_in_modified_rx = self.rtp_in_modified_rx.lock().await;
        rtp_in_modified_rx.recv().await
    }

    /// cose closes the stream and the underlying interceptor
    pub async fn close(&self) -> Result<()> {
        {
            let mut rtcp_in_tx = self.rtcp_in_tx.lock().await;
            rtcp_in_tx.take();
        }
        {
            let mut rtp_in_tx = self.rtp_in_tx.lock().await;
            rtp_in_tx.take();
        }
        self.interceptor.close().await
    }
}

#[async_trait]
impl RTCPWriter for MockStream {
    async fn write(
        &self,
        pkt: &(dyn rtcp::packet::Packet + Send + Sync),
        _attributes: &Attributes,
    ) -> Result<usize> {
        let _ = self.rtcp_out_modified_tx.send(pkt.cloned()).await;

        Ok(0)
    }
}

#[async_trait]
impl RTCPReader for MockStream {
    async fn read(&self, buf: &mut [u8], a: &Attributes) -> Result<(usize, Attributes)> {
        let pkt = {
            let mut rtcp_in = self.rtcp_in_rx.lock().await;
            rtcp_in.recv().await.ok_or(Error::ErrIoEOF)?
        };

        let marshaled = pkt.marshal()?;
        let n = marshaled.len();
        if n > buf.len() {
            return Err(Error::ErrShortBuffer.into());
        }

        buf[..n].copy_from_slice(&marshaled);
        Ok((n, a.clone()))
    }
}

#[async_trait]
impl RTPWriter for MockStream {
    async fn write(&self, pkt: &rtp::packet::Packet, _a: &Attributes) -> Result<usize> {
        let _ = self.rtp_out_modified_tx.send(pkt.clone()).await;
        Ok(0)
    }
}

#[async_trait]
impl RTPReader for MockStream {
    async fn read(&self, buf: &mut [u8], a: &Attributes) -> Result<(usize, Attributes)> {
        let pkt = {
            let mut rtp_in = self.rtp_in_rx.lock().await;
            rtp_in.recv().await.ok_or(Error::ErrIoEOF)?
        };

        let marshaled = pkt.marshal()?;
        let n = marshaled.len();
        if n > buf.len() {
            return Err(Error::ErrShortBuffer.into());
        }

        buf[..n].copy_from_slice(&marshaled);
        Ok((n, a.clone()))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::noop::NoOp;
    use rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_mock_stream() -> Result<()> {
        let s = MockStream::new(&StreamInfo::default(), Arc::new(NoOp)).await;

        s.write_rtcp(&PictureLossIndication::default()).await?;
        tokio::time::timeout(Duration::from_millis(10), s.written_rtcp()).await?;
        let result = tokio::time::timeout(Duration::from_millis(10), s.written_rtcp()).await;
        assert!(
            result.is_err(),
            "single rtcp packet written, but multiple found"
        );

        s.write_rtp(&rtp::packet::Packet::default()).await?;
        tokio::time::timeout(Duration::from_millis(10), s.written_rtp()).await?;
        let result = tokio::time::timeout(Duration::from_millis(10), s.written_rtp()).await;
        assert!(
            result.is_err(),
            "single rtp packet written, but multiple found"
        );

        s.receive_rtcp(Box::new(PictureLossIndication::default()))
            .await;
        assert!(
            tokio::time::timeout(Duration::from_millis(10), s.read_rtcp())
                .await?
                .is_some(),
            "read rtcp returned error",
        );
        let result = tokio::time::timeout(Duration::from_millis(10), s.read_rtcp()).await;
        assert!(
            result.is_err(),
            "single rtcp packet written, but multiple found"
        );

        s.receive_rtp(rtp::packet::Packet::default()).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(10), s.read_rtp())
                .await?
                .is_some(),
            "read rtp returned error",
        );
        let result = tokio::time::timeout(Duration::from_millis(10), s.read_rtp()).await;
        assert!(
            result.is_err(),
            "single rtp packet written, but multiple found"
        );

        s.close().await?;

        Ok(())
    }
}
