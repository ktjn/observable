use auth_service::session::{pkce_challenge, sign_session_jwt, verify_session_jwt};
use uuid::Uuid;

#[test]
fn round_trip_session_jwt() {
    let secret = "testsecretfortests1234567890abc";
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let session_id = Uuid::new_v4();
    let token = sign_session_jwt(
        secret,
        user_id,
        tenant_id,
        "member",
        "production",
        session_id,
    )
    .expect("sign must succeed");
    let claims = verify_session_jwt(secret, &token).expect("verify must succeed");

    assert_eq!(claims.nonce, session_id.to_string());
    assert_eq!(claims.sub, user_id.to_string());
    assert_eq!(claims.tid, tenant_id.to_string());
    assert_eq!(claims.role, "member");
    assert_eq!(claims.env, "production");
}

#[test]
fn wrong_secret_is_rejected() {
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let session_id = Uuid::new_v4();
    let token = sign_session_jwt(
        "correctsecret1234567890abcdefgh",
        user_id,
        tenant_id,
        "member",
        "prod",
        session_id,
    )
    .expect("sign");
    let result = verify_session_jwt("wrongsecretXXXXXXXXXXXXXXXXXXXX", &token);

    assert!(result.is_err(), "wrong secret must be rejected");
}

#[test]
fn session_jwt_expires_in_7_days() {
    let secret = "testsecretfortests1234567890abc";
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();

    let before = chrono::Utc::now().timestamp();
    let token = sign_session_jwt(secret, user_id, tenant_id, "member", "prod", session_id)
        .expect("sign must succeed");
    let after = chrono::Utc::now().timestamp();

    let claims = verify_session_jwt(secret, &token).expect("verify must succeed");
    let expected_min = before + 604_800 - 5; // 7 days minus tolerance
    let expected_max = after + 604_800 + 5; // 7 days plus tolerance
    assert!(
        claims.exp >= expected_min && claims.exp <= expected_max,
        "exp must be ~7 days from now, got {} (expected between {} and {})",
        claims.exp,
        expected_min,
        expected_max
    );
}

#[test]
fn pkce_challenge_is_deterministic() {
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = pkce_challenge(verifier);
    // RFC 7636 test vector: S256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
    assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
}
