// use matrix_sdk::encryption::verification::{format_emojis, Emoji, SasVerification, Verification};
// async fn wait_for_confirmation(sas: SasVerification, emoji: [Emoji; 7]) {
//     println!("\nDo the emojis match: \n{}", format_emojis(emoji));
//     print!("Confirm with `yes` or cancel with `no`: ");
//     std::io::stdout()
//         .flush()
//         .expect("We should be able to flush stdout");

//     let mut input = String::new();
//     std::io::stdin()
//         .read_line(&mut input)
//         .expect("error: unable to read user input");

//     match input.trim().to_lowercase().as_ref() {
//         "yes" | "true" | "ok" => sas.confirm().await.unwrap(),
//         _ => sas.cancel().await.unwrap(),
//     }
// }

// async fn sas_verification_handler(&self, sas: SasVerification) {
//     println!(
//         "Starting verification with {} {}",
//         &sas.other_device().user_id(),
//         &sas.other_device().device_id()
//     );
//     print_devices(sas.other_device().user_id(), &client).await;
//     sas.accept().await.unwrap();

//     let mut stream = sas.changes();

//     while let Some(state) = stream.next().await {
//         match state {
//             SasState::KeysExchanged {
//                 emojis,
//                 decimals: _,
//             } => {
//                 tokio::spawn(wait_for_confirmation(
//                     sas.clone(),
//                     emojis
//                         .expect("We only support verifications using emojis")
//                         .emojis,
//                 ));
//             }
//             SasState::Done { .. } => {
//                 let device = sas.other_device();

//                 println!(
//                     "Successfully verified device {} {} {:?}",
//                     device.user_id(),
//                     device.device_id(),
//                     device.local_trust_state()
//                 );

//                 print_devices(sas.other_device().user_id(), &client).await;

//                 break;
//             }
//             SasState::Cancelled(cancel_info) => {
//                 println!(
//                     "The verification has been cancelled, reason: {}",
//                     cancel_info.reason()
//                 );

//                 break;
//             }
//             SasState::Started { .. } | SasState::Accepted { .. } | SasState::Confirmed => (),
//         }
//     }
// }
// }
