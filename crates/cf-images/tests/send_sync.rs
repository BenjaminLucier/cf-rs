//! Compile-time checks that the client is thread-safe and every returned
//! future and stream is `Send`, so they can be spawned on a multi-threaded
//! runtime.

use cf_images::*;

fn assert_send<T: Send>(_: &T) {}
fn assert_send_sync<T: Send + Sync + 'static>() {}
fn assert_clone<T: Clone>() {}

#[test]
fn client_is_send_sync_clone() {
    assert_send_sync::<Client>();
    assert_clone::<Client>();
    assert_send_sync::<Error>();
}

#[allow(deprecated, dead_code, unused_must_use)]
fn futures_are_send(client: &Client, id: &ImageId, variant: &VariantName, key: &SigningKeyName) {
    let images = client.images();
    assert_send(&images.upload(UploadSource::bytes(vec![], "a")).send());
    assert_send(&images.get(id));
    assert_send(&images.update(id, UpdateImage::new()));
    assert_send(&images.delete(id));
    assert_send(&images.blob(id));
    assert_send(&images.metadata_keys());
    let list = images.list();
    assert_send(&list.send());
    assert_send(&images.list_all());
    assert_send(&images.list().into_stream());
    let list_v1 = images.list_v1();
    assert_send(&list_v1.send());

    let variants = client.variants();
    assert_send(&variants.list());
    assert_send(&variants.get(variant));
    assert_send(&variants.create(NewVariant::new("v", VariantOptions::new(Fit::Cover, 1, 1))));
    assert_send(&variants.update(
        variant,
        UpdateVariant::new(VariantOptions::new(Fit::Cover, 1, 1)),
    ));
    assert_send(&variants.delete(variant));

    let keys = client.signing_keys();
    assert_send(&keys.list());
    assert_send(&keys.create(key));
    assert_send(&keys.delete(key));

    assert_send(&client.stats());
    assert_send(&client.direct_uploads().create().send());
}

#[test]
fn compiles() {
    // `futures_are_send` only needs to type-check.
    let _ = futures_are_send;
}
