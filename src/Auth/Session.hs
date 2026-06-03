{-# LANGUAGE FlexibleContexts #-}
{-# LANGUAGE TypeOperators    #-}

module Auth.Session
  ( AuthedUser (..)
  , sessionAuthHandler
  , createSession
  , lookupValidSession
  , extractToken
  ) where

import           Control.Monad.Except                 (throwError)
import           Control.Monad.IO.Class               (liftIO)
import           Control.Monad.Reader                 (asks)
import qualified Data.ByteString                      as BS
import qualified Data.ByteString.Char8                as BC
import           Data.Text                            (Text)
import qualified Data.Text.Encoding                   as TE
import           Data.Time                            (addUTCTime, getCurrentTime)
import           Database.Persist
import           Database.Persist.Sql                 (SqlBackend, SqlPersistT,
                                                       runSqlPool)
import           Data.Pool                            (Pool)
import           Network.Wai                          (Request, requestHeaders)
import           Servant                              (err401, errBody)
import           Servant.API.Experimental.Auth        (AuthProtect)
import           Servant.Server                       (Handler)
import           Servant.Server.Experimental.Auth     (AuthHandler, AuthServerData,
                                                       mkAuthHandler)
import           Web.Cookie                           (parseCookies)

import           Auth.Config                          (Config (..))
import           Auth.Crypto.Token                    (randomToken)
import           Auth.Models
import           Auth.Types                           (AppEnv (..), AppM, runDB)

data AuthedUser = AuthedUser
  { auUserId  :: UserId
  , auUser    :: User
  , auSession :: Entity Session
  }

type instance AuthServerData (AuthProtect "session") = AuthedUser

createSession :: UserId -> Bool -> AppM Text
createSession uid mfaPending = do
  cfg <- asks envConfig
  tok <- liftIO (randomToken 32)
  now <- liftIO getCurrentTime
  let expires = addUTCTime (fromIntegral (cfgSessionTtlSeconds cfg)) now
  _ <- runDB $ insert (Session uid tok mfaPending expires now)
  pure tok

lookupValidSession
  :: Text
  -> SqlPersistT IO (Maybe (Entity Session, User))
lookupValidSession tok = do
  now   <- liftIO getCurrentTime
  msess <- getBy (UniqueSessionToken tok)
  case msess of
    Nothing -> pure Nothing
    Just esess@(Entity _ sess)
      | sessionExpiresAt sess < now -> pure Nothing
      | otherwise -> do
          muser <- get (sessionUserId sess)
          pure $ fmap (\u -> (esess, u)) muser

extractToken :: Request -> Maybe Text
extractToken req =
  case lookup "authorization" hs of
    Just v | bearer `BS.isPrefixOf` v -> Just (TE.decodeUtf8 (BS.drop (BS.length bearer) v))
    _ -> do
      cookieHeader <- lookup "cookie" hs
      tok <- lookup "session" (parseCookies cookieHeader)
      pure (TE.decodeUtf8 tok)
  where
    hs     = requestHeaders req
    bearer = BC.pack "Bearer "

sessionAuthHandler :: AppEnv -> AuthHandler Request AuthedUser
sessionAuthHandler env = mkAuthHandler handler
  where
    pool :: Pool SqlBackend
    pool = envPool env

    handler :: Request -> Handler AuthedUser
    handler req = do
      tok <- maybe (throwError (unauth "missing session token")) pure (extractToken req)
      res <- liftIO $ runSqlPool (lookupValidSession tok) pool
      case res of
        Nothing -> throwError (unauth "invalid or expired session")
        Just (esess@(Entity _ sess), user)
          | sessionMfaPending sess -> throwError (unauth "MFA not completed for this session")
          | otherwise -> pure (AuthedUser (sessionUserId sess) user esess)

    unauth msg = err401 { errBody = msg }
