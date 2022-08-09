// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use crate::quorum_store::{quorum_store::QuorumStoreError, types::BatchId, utils::DigestTimeouts};
use aptos_crypto::HashValue;
use aptos_logger::{debug, info};
use aptos_types::validator_verifier::ValidatorVerifier;
use consensus_types::proof_of_store::{ProofOfStore, SignedDigest, SignedDigestError, SignedDigestInfo};
use futures::channel::oneshot;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tokio::{time, sync::oneshot as TokioOneshot};
use aptos_types::PeerId;

#[derive(Debug)]
pub(crate) enum ProofBuilderCommand {
    InitProof(SignedDigestInfo, BatchId, ProofReturnChannel),
    AppendSignature(SignedDigest),
    Shutdown(TokioOneshot::Sender<()>),
}

pub(crate) type ProofReturnChannel =
oneshot::Sender<Result<(ProofOfStore, BatchId), QuorumStoreError>>;

pub(crate) struct ProofBuilder {
    peer_id: PeerId,
    proof_timeout_ms: usize,
    digest_to_proof: HashMap<HashValue, (ProofOfStore, BatchId, ProofReturnChannel)>,
    timeouts: DigestTimeouts,
}

//PoQS builder object - gather signed digest to form PoQS
impl ProofBuilder {
    pub fn new(proof_timeout_ms: usize, peer_id: PeerId) -> Self {
        Self {
            peer_id,
            proof_timeout_ms,
            digest_to_proof: HashMap::new(),
            timeouts: DigestTimeouts::new(),
        }
    }

    fn init_proof(
        &mut self,
        info: SignedDigestInfo,
        batch_id: BatchId,
        tx: ProofReturnChannel,
    ) -> Result<(), SignedDigestError> {
        self.timeouts.add_digest(info.digest, self.proof_timeout_ms);
        self.digest_to_proof
            .insert(info.digest, (ProofOfStore::new(info), batch_id, tx));
        Ok(())
    }

    fn add_signature(
        &mut self,
        signed_digest: SignedDigest,
        validator_verifier: &ValidatorVerifier,
    ) -> Result<(), SignedDigestError> {
        if !self
            .digest_to_proof
            .contains_key(&signed_digest.info.digest)
        {
            return Err(SignedDigestError::WrongDigest);
        }
        let mut ret = Ok(());
        let mut ready = false;
        let digest = signed_digest.info.digest.clone();
        let my_id = self.peer_id;
        self.digest_to_proof
            .entry(signed_digest.info.digest)
            .and_modify(|(proof, _, _)| {
                ret = proof.add_signature(signed_digest.peer_id, signed_digest.signature);
                if ret.is_ok() {
                    ready = proof.ready(validator_verifier, my_id);
                }
            });
        if ready {
            let (proof, batch_id, tx) = self.digest_to_proof.remove(&digest).unwrap();
            tx.send(Ok((proof, batch_id)))
                .expect("Unable to send the proof of store");
        }
        ret
    }

    fn expire(&mut self) {
        for digest in self.timeouts.expire() {
            if let Some((_, batch_id, tx)) = self.digest_to_proof.remove(&digest) {
                tx.send(Err(QuorumStoreError::Timeout(batch_id)))
                    .expect("Unable to send the timeout a proof of store");
            }
        }
    }

    pub async fn start(
        mut self,
        mut rx: Receiver<ProofBuilderCommand>,
        validator_verifier: ValidatorVerifier,
    ) {
        let mut interval = time::interval(Duration::from_millis(100));
        loop {
            tokio::select! {
             Some(command) = rx.recv() => {
                match command {
                        ProofBuilderCommand::Shutdown(ack_tx) => {
                    ack_tx
                        .send(())
                        .expect("Failed to send shutdown ack to QuorumStore");
                    break;
                }
                    ProofBuilderCommand::InitProof(info, batch_id, tx) => {
                        self.init_proof(info, batch_id, tx)
                            .expect("Error initializing proof of store");
                    }
                    ProofBuilderCommand::AppendSignature(signed_digest) => {
                            let peer_id = signed_digest.peer_id;
                        if let Err(e) = self.add_signature(signed_digest, &validator_verifier) {
                            // Can happen if we already garbage collected
                            if peer_id == self.peer_id {
                                info!("QS: could not add signature from self, err = {:?}", e);
                                }
                        } else {
                            debug!("QS: added signature to proof");
                        }
                    }
                }

                }
                _ = interval.tick() => {
                    self.expire();
                }
            }
        }
    }
}