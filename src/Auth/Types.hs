module Auth.Types
  ( AppEnv (..)
  , AppM
  , runDB
  , passwordParams
  ) where

import           Control.Monad.IO.Class  (liftIO)
import           Control.Monad.Reader    (ReaderT, asks)
import           Data.Pool               (Pool)
import           Database.Persist.Sql    (SqlBackend, SqlPersistT, runSqlPool)
import           Servant                 (Handler)

import           Auth.Config             (Config (..))
import           Auth.Crypto.Password    (PasswordParams (..))

data AppEnv = AppEnv
  { envPool   :: Pool SqlBackend
  , envConfig :: Config
  }

type AppM = ReaderT AppEnv Handler

runDB :: SqlPersistT IO a -> AppM a
runDB action = do
  pool <- asks envPool
  liftIO $ runSqlPool action pool

passwordParams :: Config -> PasswordParams
passwordParams cfg = PasswordParams
  { ppPepper     = cfgPasswordPepper cfg
  , ppIterations = cfgPbkdf2Iterations cfg
  }
