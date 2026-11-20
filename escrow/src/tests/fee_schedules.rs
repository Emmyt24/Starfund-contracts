use super::{free_addresses, setup};
use crate::{
    FeeSchedule, FeeScheduleActivated, FeeScheduleSubmitted, StarfundEscrowClient,
    StarfundEscrowError,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, String,
};

fn init_fee_schedule_test<'a>(env: &'a Env) -> (StarfundEscrowClient<'a>, Address) {
    let (client, admin, sme) = setup(env);
    let (funding_token, treasury) = free_addresses(env);
    client.init(
        &admin,
        &String::from_str(env, "FEES01"),
        &sme,
        &100_000i128,
        &800i64,
        &0u64,
        &funding_token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    (client, admin)
}

#[test]
fn fee_schedule_changes_emit_events() {
    let env = Env::default();
    let (client, _admin) = init_fee_schedule_test(&env);
    let contract_id = client.address.clone();
    let first = FeeSchedule {
        fee_bps: 100,
        min_fee_bps: 50,
        max_fee_bps: 150,
        activation_ledger: 110,
    };

    client.submit_fee_schedule(&first);
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        FeeScheduleSubmitted {
            name: symbol_short!("fee_sub"),
            invoice_id: client.get_escrow().invoice_id,
            schedule: first.clone(),
        }
        .to_xdr(&env, &contract_id)
    );

    let event_count = env.events().all().events().len();
    client.submit_fee_schedule(&first);
    assert_eq!(env.events().all().events().len(), event_count);

    let mut ledger = env.ledger().get();
    ledger.sequence_number = first.activation_ledger;
    env.ledger().set(ledger);
    assert!(client.activate_fee_schedule());
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        FeeScheduleActivated {
            name: symbol_short!("fee_act"),
            invoice_id: client.get_escrow().invoice_id,
            schedule: first.clone(),
            previous: None,
        }
        .to_xdr(&env, &contract_id)
    );

    let second = FeeSchedule {
        fee_bps: 200,
        min_fee_bps: 100,
        max_fee_bps: 250,
        activation_ledger: 120,
    };
    client.submit_fee_schedule(&second);
    let mut ledger = env.ledger().get();
    ledger.sequence_number = second.activation_ledger;
    env.ledger().set(ledger);
    assert!(client.activate_fee_schedule());
    assert_eq!(
        env.events().all().events().last().unwrap().clone(),
        FeeScheduleActivated {
            name: symbol_short!("fee_act"),
            invoice_id: client.get_escrow().invoice_id,
            schedule: second,
            previous: Some(first),
        }
        .to_xdr(&env, &contract_id)
    );
}

#[test]
fn non_admin_submission_leaves_pending_state_unchanged() {
    let env = Env::default();
    let (client, _admin) = init_fee_schedule_test(&env);
    let (non_admin, _) = free_addresses(&env);
    let schedule = FeeSchedule {
        fee_bps: 100,
        min_fee_bps: 50,
        max_fee_bps: 150,
        activation_ledger: 110,
    };

    // Empty auth: no signer at all.
    env.mock_auths(&[]);
    let result = client.try_submit_fee_schedule(&schedule);
    assert_eq!(result, Err(Ok(StarfundEscrowError::Unauthorized)));
    assert_eq!(client.get_pending_fee_schedule(), None);

    // Exact non-admin auth fixture: signer is not the admin.
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "submit_fee_schedule",
            args: (&schedule,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_submit_fee_schedule(&schedule);
    assert_eq!(result, Err(Ok(StarfundEscrowError::Unauthorized)));
    assert_eq!(client.get_pending_fee_schedule(), None);
}

#[test]
fn admin_submission_succeeds_and_stores_pending_schedule() {
    let env = Env::default();
    let (client, admin) = init_fee_schedule_test(&env);
    let schedule = FeeSchedule {
        fee_bps: 100,
        min_fee_bps: 50,
        max_fee_bps: 150,
        activation_ledger: 110,
    };

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "submit_fee_schedule",
            args: (&schedule,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.submit_fee_schedule(&schedule);
    assert_eq!(client.get_pending_fee_schedule(), Some(schedule));
}

#[test]
fn anyone_can_activate_authorized_due_schedule() {
    let env = Env::default();
    let (client, admin) = init_fee_schedule_test(&env);
    let (non_admin, _) = free_addresses(&env);
    let schedule = FeeSchedule {
        fee_bps: 100,
        min_fee_bps: 50,
        max_fee_bps: 150,
        activation_ledger: 110,
    };

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "submit_fee_schedule",
            args: (&schedule,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.submit_fee_schedule(&schedule);

    let mut ledger = env.ledger().get();
    ledger.sequence_number = schedule.activation_ledger;
    env.ledger().set(ledger);

    // Empty auth: activation requires no authorization.
    env.mock_auths(&[]);
    assert!(client.activate_fee_schedule());
    assert_eq!(client.get_fee_schedule(), Some(schedule.clone()));

    // Exact non-admin auth fixture also succeeds.
    let next = FeeSchedule {
        fee_bps: 200,
        min_fee_bps: 100,
        max_fee_bps: 250,
        activation_ledger: 120,
    };
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "submit_fee_schedule",
            args: (&next,).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.submit_fee_schedule(&next);

    let mut ledger = env.ledger().get();
    ledger.sequence_number = next.activation_ledger;
    env.ledger().set(ledger);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &client.address,
            fn_name: "activate_fee_schedule",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(client.activate_fee_schedule());
    assert_eq!(client.get_fee_schedule(), Some(next));
}
