-- | Database connection pool and migrations.
module Auth.Database
  ( mkPool
  , runMigrations
  ) where

import           Control.Monad.Logger        (runStdoutLoggingT)
import           Data.Pool                   (Pool)
import           Database.Persist.Postgresql (createPostgresqlPool, runMigration,
                                              runSqlPool)
import           Database.Persist.Sql        (SqlBackend)

import           Auth.Config                 (Config (..))
import           Auth.Models                 (migrateAll)

mkPool :: Config -> IO (Pool SqlBackend)
mkPool cfg = runStdoutLoggingT $ createPostgresqlPool (cfgDatabaseUrl cfg) 10

runMigrations :: Pool SqlBackend -> IO ()
runMigrations pool = runStdoutLoggingT $ runSqlPool (runMigration migrateAll) pool
