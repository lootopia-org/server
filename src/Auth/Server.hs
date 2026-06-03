module Auth.Server
  ( runServer
  ) where

import           Data.Text.Encoding                   (encodeUtf8)
import           Network.Wai.Handler.Warp             (run)
import           Network.Wai.Middleware.Cors          (CorsResourcePolicy (..),
                                                       cors,
                                                       simpleCorsResourcePolicy)
import           Network.Wai.Middleware.RequestLogger (logStdoutDev)

import           Auth.Api                             (mkApplication)
import           Auth.Config
import           Auth.Database                        (mkPool)
import           Auth.Types                           (AppEnv (..))

runServer :: IO ()
runServer = do
  loadDotEnv ".env"
  cfg  <- loadConfig
  pool <- mkPool cfg
  let env = AppEnv pool cfg
      corsPolicy = simpleCorsResourcePolicy
        { corsOrigins        = Just ([encodeUtf8 (cfgOrigin cfg)], True)
        , corsMethods        = ["GET", "POST", "OPTIONS"]
        , corsRequestHeaders = ["Content-Type", "Authorization"]
        }
      middleware = logStdoutDev . cors (const (Just corsPolicy))
  putStrLn $ "haskell-auth-server listening on http://localhost:" <> show (cfgPort cfg)
  run (cfgPort cfg) (middleware (mkApplication env))
