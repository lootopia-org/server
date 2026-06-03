{-# LANGUAGE DeriveGeneric       #-}
{-# LANGUAGE FlexibleContexts    #-}
{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeApplications    #-}
{-# LANGUAGE TypeOperators       #-}

module Auth.Api
  ( API
  , mkApplication
  ) where

import           Control.Monad                    (unless, when)
import           Control.Monad.IO.Class           (liftIO)
import           Control.Monad.Reader             (asks, runReaderT)
import           Data.Aeson                       (FromJSON (..), ToJSON (..),
                                                   Value, defaultOptions,
                                                   genericParseJSON,
                                                   genericToJSON)
import           Data.Aeson.Types                 (Options (..))
import           Data.Char                        (toLower)
import           Data.Int                         (Int64)
import           Data.Maybe                       (isJust)
import           Data.Text                        (Text)
import           Data.Time                        (UTCTime, addUTCTime,
                                                   getCurrentTime)
import           Database.Persist
import           Database.Persist.Sql             (fromSqlKey)
import           GHC.Generics                     (Generic)
import           Network.Wai                      (Request)
import           Servant

import           Auth.Config                      (Config (..))
import           Auth.Crypto.Password             (StoredPassword (..),
                                                   hashNewPassword,
                                                   verifyPassword)
import           Auth.Crypto.Token                (randomBytes, randomToken)
import           Auth.Crypto.Totp                 (generateSecret, otpauthUri,
                                                   secretToBase32, verifyCode)
import           Auth.Email                       (sendVerificationEmail)
import           Auth.Models
import           Auth.Session                     (AuthedUser (..),
                                                   createSession,
                                                   sessionAuthHandler)
import           Auth.Types                       (AppEnv (..), AppM,
                                                   passwordParams, runDB)
import qualified Auth.Webauthn                    as WebAuthn
import           Servant.Server.Experimental.Auth (AuthHandler)

jsonOpts :: String -> Options
jsonOpts prefix = defaultOptions { fieldLabelModifier = lower1 . drop (length prefix) }
  where
    lower1 []     = []
    lower1 (c:cs) = toLower c : cs

data RegisterReq = RegisterReq { rqEmail :: Text, rqPassword :: Text } deriving (Generic)
instance FromJSON RegisterReq where parseJSON = genericParseJSON (jsonOpts "rq")

newtype EmailReq = EmailReq { erEmail :: Text } deriving (Generic)
instance FromJSON EmailReq where parseJSON = genericParseJSON (jsonOpts "er")

data LoginReq = LoginReq { lqEmail :: Text, lqPassword :: Text } deriving (Generic)
instance FromJSON LoginReq where parseJSON = genericParseJSON (jsonOpts "lq")

data MfaTotpReq = MfaTotpReq { mtToken :: Text, mtCode :: Text } deriving (Generic)
instance FromJSON MfaTotpReq where parseJSON = genericParseJSON (jsonOpts "mt")

newtype CodeReq = CodeReq { crCode :: Text } deriving (Generic)
instance FromJSON CodeReq where parseJSON = genericParseJSON (jsonOpts "cr")

data WebauthnCompleteReq = WebauthnCompleteReq
  { wcHandle     :: Text
  , wcCredential :: Value
  } deriving (Generic)
instance FromJSON WebauthnCompleteReq where parseJSON = genericParseJSON (jsonOpts "wc")

newtype MessageResp = MessageResp { msgMessage :: Text } deriving (Generic)
instance ToJSON MessageResp where toJSON = genericToJSON (jsonOpts "msg")

newtype TokenResp = TokenResp { tkToken :: Text } deriving (Generic)
instance ToJSON TokenResp where toJSON = genericToJSON (jsonOpts "tk")

data LoginResp = LoginResp
  { lpToken       :: Text
  , lpMfaRequired :: Bool
  , lpMfaMethods  :: [Text]
  } deriving (Generic)
instance ToJSON LoginResp where toJSON = genericToJSON (jsonOpts "lp")

data MeResp = MeResp
  { meId            :: Int64
  , meEmail         :: Text
  , meEmailVerified :: Bool
  , meTotpEnabled   :: Bool
  , mePasskeys      :: Int
  } deriving (Generic)
instance ToJSON MeResp where toJSON = genericToJSON (jsonOpts "me")

data TotpEnrollResp = TotpEnrollResp
  { teSecret     :: Text
  , teOtpauthUri :: Text
  } deriving (Generic)
instance ToJSON TotpEnrollResp where toJSON = genericToJSON (jsonOpts "te")

data CredentialResp = CredentialResp
  { cdId        :: Int64
  , cdCreatedAt :: UTCTime
  } deriving (Generic)
instance ToJSON CredentialResp where toJSON = genericToJSON (jsonOpts "cd")


type API =
       "auth" :> "register"            :> ReqBody '[JSON] RegisterReq :> Post '[JSON] MessageResp
  :<|> "auth" :> "verify-email"        :> QueryParam "token" Text     :> Get  '[JSON] MessageResp
  :<|> "auth" :> "resend-verification" :> ReqBody '[JSON] EmailReq    :> Post '[JSON] MessageResp
  :<|> "auth" :> "login"               :> ReqBody '[JSON] LoginReq    :> Post '[JSON] LoginResp
  :<|> "auth" :> "mfa" :> "totp"       :> ReqBody '[JSON] MfaTotpReq  :> Post '[JSON] TokenResp
  :<|> "auth" :> "webauthn" :> "login" :> "begin"
                                       :> ReqBody '[JSON] EmailReq    :> Post '[JSON] Value
  :<|> "auth" :> "webauthn" :> "login" :> "complete"
                                       :> ReqBody '[JSON] WebauthnCompleteReq :> Post '[JSON] TokenResp
  :<|> AuthProtect "session" :> ProtectedAPI

type ProtectedAPI =
       "me"                                  :> Get  '[JSON] MeResp
  :<|> "auth" :> "logout"                    :> Post '[JSON] MessageResp
  :<|> "auth" :> "totp" :> "enroll" :> "begin"  :> Post '[JSON] TotpEnrollResp
  :<|> "auth" :> "totp" :> "enroll" :> "verify" :> ReqBody '[JSON] CodeReq :> Post '[JSON] MessageResp
  :<|> "auth" :> "totp" :> "disable"         :> ReqBody '[JSON] CodeReq :> Post '[JSON] MessageResp
  :<|> "auth" :> "webauthn" :> "register" :> "begin"    :> Post '[JSON] Value
  :<|> "auth" :> "webauthn" :> "register" :> "complete" :> ReqBody '[JSON] WebauthnCompleteReq :> Post '[JSON] MessageResp
  :<|> "auth" :> "webauthn" :> "credentials" :> Get '[JSON] [CredentialResp]


server :: ServerT API AppM
server =
       registerH
  :<|> verifyEmailH
  :<|> resendH
  :<|> loginH
  :<|> mfaTotpH
  :<|> WebAuthn.beginAuthentication . erEmail
  :<|> webauthnLoginCompleteH
  :<|> protectedServer

protectedServer :: AuthedUser -> ServerT ProtectedAPI AppM
protectedServer u =
       meH u
  :<|> logoutH u
  :<|> totpEnrollBeginH u
  :<|> totpEnrollVerifyH u
  :<|> totpDisableH u
  :<|> webauthnRegisterBeginH u
  :<|> webauthnRegisterCompleteH u
  :<|> credentialsH u

ok :: Text -> AppM MessageResp
ok = pure . MessageResp


registerH :: RegisterReq -> AppM MessageResp
registerH RegisterReq{..} = do
  cfg <- asks envConfig
  existing <- runDB $ getBy (UniqueUserEmail rqEmail)
  when (isJust existing) $ throwError err409 { errBody = "email already registered" }
  when (rqPassword == "") $ throwError err400 { errBody = "password must not be empty" }
  stored <- liftIO $ hashNewPassword (passwordParams cfg) rqPassword
  uh     <- liftIO $ randomBytes 16
  now    <- liftIO getCurrentTime
  uid <- runDB $ insert User
    { userEmail        = rqEmail
    , userEmailVerified = False
    , userPasswordSalt = spSalt stored
    , userPasswordHash = spHash stored
    , userUserHandle   = uh
    , userTotpSecret   = Nothing
    , userTotpEnabled  = False
    , userCreatedAt    = now
    }
  issueVerification cfg uid rqEmail
  ok "registered; check your email to verify your address"

issueVerification :: Config -> UserId -> Text -> AppM ()
issueVerification cfg uid email = do
  tok <- liftIO $ randomToken 32
  now <- liftIO getCurrentTime
  let expires = addUTCTime (fromIntegral (cfgEmailVerifyTtlSeconds cfg)) now
  runDB $ do
    deleteWhere [EmailTokenUserId ==. uid]
    insert_ (EmailToken uid tok expires)
  let link = cfgPublicBaseUrl cfg <> "/auth/verify-email?token=" <> tok
  liftIO $ sendVerificationEmail cfg email link

verifyEmailH :: Maybe Text -> AppM MessageResp
verifyEmailH Nothing = throwError err400 { errBody = "missing token" }
verifyEmailH (Just tok) = do
  now  <- liftIO getCurrentTime
  mtok <- runDB $ getBy (UniqueEmailToken tok)
  case mtok of
    Just (Entity tid et)
      | emailTokenExpiresAt et >= now -> do
          runDB $ do
            update (emailTokenUserId et) [UserEmailVerified =. True]
            delete tid
          ok "email verified"
    _ -> throwError err400 { errBody = "invalid or expired token" }

resendH :: EmailReq -> AppM MessageResp
resendH EmailReq{..} = do
  cfg   <- asks envConfig
  muser <- runDB $ getBy (UniqueUserEmail erEmail)
  case muser of
    Just (Entity uid user) | not (userEmailVerified user) -> issueVerification cfg uid erEmail
    _ -> pure ()  -- do not reveal whether the address exists / is verified
  ok "if the address exists and is unverified, a new link has been sent"


loginH :: LoginReq -> AppM LoginResp
loginH LoginReq{..} = do
  cfg   <- asks envConfig
  muser <- runDB $ getBy (UniqueUserEmail lqEmail)
  case muser of
    Nothing -> throwError invalidCreds
    Just (Entity uid user) -> do
      let stored = StoredPassword (userPasswordSalt user) (userPasswordHash user)
      unless (verifyPassword (passwordParams cfg) lqPassword stored) $ throwError invalidCreds
      when (cfgRequireVerifiedEmail cfg && not (userEmailVerified user)) $
        throwError err403 { errBody = "email_not_verified" }
      let mfaNeeded = userTotpEnabled user
      tok <- createSession uid mfaNeeded
      pure LoginResp
        { lpToken       = tok
        , lpMfaRequired = mfaNeeded
        , lpMfaMethods  = if mfaNeeded then ["totp"] else []
        }
  where
    invalidCreds = err401 { errBody = "invalid email or password" }

mfaTotpH :: MfaTotpReq -> AppM TokenResp
mfaTotpH MfaTotpReq{..} = do
  now   <- liftIO getCurrentTime
  msess <- runDB $ getBy (UniqueSessionToken mtToken)
  case msess of
    Just (Entity sid sess) | sessionExpiresAt sess >= now -> do
      muser <- runDB $ get (sessionUserId sess)
      case muser >>= userTotpSecret of
        Nothing -> throwError err400 { errBody = "TOTP is not enabled for this account" }
        Just secret -> do
          valid <- liftIO $ verifyCode secret mtCode
          unless valid $ throwError err401 { errBody = "invalid code" }
          runDB $ update sid [SessionMfaPending =. False]
          pure (TokenResp mtToken)
    _ -> throwError err401 { errBody = "invalid or expired session" }

webauthnLoginCompleteH :: WebauthnCompleteReq -> AppM TokenResp
webauthnLoginCompleteH WebauthnCompleteReq{..} = do
  uid <- WebAuthn.completeAuthentication wcHandle wcCredential
  tok <- createSession uid False   
  pure (TokenResp tok)


meH :: AuthedUser -> AppM MeResp
meH AuthedUser{..} = do
  n <- runDB $ count [CredentialUserId ==. auUserId]
  pure MeResp
    { meId            = fromSqlKey auUserId
    , meEmail         = userEmail auUser
    , meEmailVerified = userEmailVerified auUser
    , meTotpEnabled   = userTotpEnabled auUser
    , mePasskeys      = n
    }

logoutH :: AuthedUser -> AppM MessageResp
logoutH AuthedUser{..} = do
  runDB $ delete (entityKey auSession)
  ok "logged out"

totpEnrollBeginH :: AuthedUser -> AppM TotpEnrollResp
totpEnrollBeginH AuthedUser{..} = do
  cfg    <- asks envConfig
  secret <- liftIO generateSecret
  runDB $ update auUserId [UserTotpSecret =. Just secret, UserTotpEnabled =. False]
  pure TotpEnrollResp
    { teSecret     = secretToBase32 secret
    , teOtpauthUri = otpauthUri (cfgRpName cfg) (userEmail auUser) secret
    }

totpEnrollVerifyH :: AuthedUser -> CodeReq -> AppM MessageResp
totpEnrollVerifyH AuthedUser{..} CodeReq{..} =
  case userTotpSecret auUser of
    Nothing -> throwError err400 { errBody = "no TOTP enrollment in progress" }
    Just secret -> do
      valid <- liftIO $ verifyCode secret crCode
      unless valid $ throwError err401 { errBody = "invalid code" }
      runDB $ update auUserId [UserTotpEnabled =. True]
      ok "TOTP enabled"

totpDisableH :: AuthedUser -> CodeReq -> AppM MessageResp
totpDisableH AuthedUser{..} CodeReq{..} =
  case userTotpSecret auUser of
    Nothing -> throwError err400 { errBody = "TOTP is not enabled" }
    Just secret -> do
      valid <- liftIO $ verifyCode secret crCode
      unless valid $ throwError err401 { errBody = "invalid code" }
      runDB $ update auUserId [UserTotpEnabled =. False, UserTotpSecret =. Nothing]
      ok "TOTP disabled"

webauthnRegisterBeginH :: AuthedUser -> AppM Value
webauthnRegisterBeginH AuthedUser{..} = WebAuthn.beginRegistration auUserId auUser

webauthnRegisterCompleteH :: AuthedUser -> WebauthnCompleteReq -> AppM MessageResp
webauthnRegisterCompleteH AuthedUser{..} WebauthnCompleteReq{..} = do
  WebAuthn.completeRegistration auUserId auUser wcHandle wcCredential
  ok "passkey registered"

credentialsH :: AuthedUser -> AppM [CredentialResp]
credentialsH AuthedUser{..} = do
  creds <- runDB $ selectList [CredentialUserId ==. auUserId] []
  pure [ CredentialResp (fromSqlKey k) (credentialCreatedAt c) | Entity k c <- creds ]


apiProxy :: Proxy API
apiProxy = Proxy

contextProxy :: Proxy '[AuthHandler Request AuthedUser]
contextProxy = Proxy

mkApplication :: AppEnv -> Application
mkApplication env =
  serveWithContext apiProxy ctx $
    hoistServerWithContext apiProxy contextProxy nt server
  where
    ctx = sessionAuthHandler env :. EmptyContext
    nt :: AppM a -> Handler a
    nt action = runReaderT action env
