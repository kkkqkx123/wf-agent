#[macro_export]
macro_rules! make_base_adapter {
    (
        $name:ident,
        $entity:ty,
        $list_ty:ty
    ) => {
        pub struct $name<S: $crate::domain::Store> {
            entity_store: $crate::store::EntityStore<S, $entity>,
        }

        impl<S: $crate::domain::Store> $name<S> {
            pub fn new(storage: S) -> Self {
                Self {
                    entity_store: $crate::store::EntityStore::new(storage),
                }
            }

            pub fn entity_store(&self) -> &$crate::store::EntityStore<S, $entity> {
                &self.entity_store
            }

            pub fn into_inner(self) -> $crate::store::EntityStore<S, $entity> {
                self.entity_store
            }

            pub fn store(&self) -> &S {
                self.entity_store.inner()
            }
        }

        impl<S: $crate::domain::Store> $crate::adapter::base::BaseStorageAdapter<$entity, $list_ty>
            for $name<S>
        {
            async fn initialize(&self) -> Result<(), $crate::error::StorageError> {
                Ok(())
            }

            async fn close(&self) -> Result<(), $crate::error::StorageError> {
                Ok(())
            }

                async fn save(
                    &self,
                    entity: &$entity,
                ) -> Result<(), $crate::error::StorageError> {
                    self.entity_store.save(entity).await
                }

                async fn load(
                    &self,
                    id: &str,
                ) -> Result<Option<$entity>, $crate::error::StorageError> {
                    self.entity_store.load(id).await
                }

                async fn delete(
                    &self,
                    id: &str,
                ) -> Result<bool, $crate::error::StorageError> {
                    let existed = self.entity_store.exists(id).await?;
                    self.entity_store.delete(id).await?;
                    Ok(existed)
                }

                async fn list(
                    &self,
                    options: Option<$list_ty>,
                ) -> Result<Vec<$entity>, $crate::error::StorageError> {
                    let filter: Option<$crate::domain::QueryFilter> =
                        options.map(Into::into);
                    self.entity_store.list(filter.as_ref()).await
                }

                async fn clear(&self) -> Result<(), $crate::error::StorageError> {
                    self.entity_store.clear().await
                }

                async fn exists(
                    &self,
                    id: &str,
                ) -> Result<bool, $crate::error::StorageError> {
                    self.entity_store.exists(id).await
                }
        }
    };
}
