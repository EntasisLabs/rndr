
#[cfg(test)]
mod extra_review_tests {
 use super::*;
 #[test]
 fn exhausted_lease_admission_at_capacity_preserves_registry() {
  let (mut f, root)=Coordinator::new(Limits { leases: 1, ..Limits::default() }).unwrap();
  let doc=f.create_document().unwrap();
  f.lease_serial=u64::MAX-1;
  let lease=f.lease(root.id(),doc).unwrap();
  drop(lease);
  let before=f.counts();
  assert_eq!(f.lease(root.id(),doc).err(),Some(Error::Exhausted));
  assert_eq!(f.counts(),before,"F0 §3 exhaustion before partial mutation");
 }
}
